//! Microphone capture with a simple energy-based end-of-speech detector.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::signal::rms;
use crate::{AudioClip, AudioError};

/// How a recording ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecordOptions {
    /// Stop on its own after speech followed by this much silence.
    pub auto_stop_after: Option<Duration>,
    /// Give up when no speech starts within this time.
    pub idle_timeout: Option<Duration>,
    /// Hard limit on recording length.
    pub max_duration: Duration,
}

impl Default for RecordOptions {
    fn default() -> Self {
        Self {
            auto_stop_after: Some(Duration::from_millis(1_400)),
            idle_timeout: None,
            max_duration: Duration::from_secs(90),
        }
    }
}

/// Minimum level counted as speech, whatever the noise floor.
const MIN_SPEECH_LEVEL: f32 = 0.015;

#[derive(Default)]
struct Shared {
    samples: Mutex<Vec<f32>>,
    /// Latest block level (f32 bits), for the UI meter.
    level: AtomicU32,
    stop: AtomicBool,
    /// Set when the recorder stopped by itself (silence or time limit).
    finished: AtomicBool,
    heard_speech: AtomicBool,
}

/// A recording in progress. The capture stream lives on its own thread
/// because audio streams cannot move between threads on every platform.
pub struct Recording {
    shared: Arc<Shared>,
    sample_rate: u32,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Recording {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recording")
            .field("sample_rate", &self.sample_rate)
            .finish_non_exhaustive()
    }
}

impl Recording {
    /// Starts recording from the default microphone.
    pub fn start(options: RecordOptions) -> Result<Self, AudioError> {
        let shared = Arc::new(Shared::default());
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, AudioError>>();
        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("kg-capture".into())
            .spawn(move || capture_thread(thread_shared, options, ready_tx))
            .map_err(|e| AudioError::Device(e.to_string()))?;
        let sample_rate = ready_rx
            .recv()
            .map_err(|_| AudioError::Device("capture thread ended early".into()))??;
        Ok(Self {
            shared,
            sample_rate,
            thread: Some(thread),
        })
    }

    /// Current input level, roughly `0.0..=1.0`.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.shared.level.load(Ordering::Relaxed))
    }

    /// True when the recorder stopped by itself.
    pub fn is_finished(&self) -> bool {
        self.shared.finished.load(Ordering::Relaxed)
    }

    /// True once something louder than the room noise was heard.
    pub fn heard_speech(&self) -> bool {
        self.shared.heard_speech.load(Ordering::Relaxed)
    }

    /// Stops recording and returns the captured audio.
    pub fn finish(mut self) -> AudioClip {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let samples = self
            .shared
            .samples
            .lock()
            .map(|mut s| std::mem::take(&mut *s))
            .unwrap_or_default();
        AudioClip {
            samples,
            sample_rate: self.sample_rate,
        }
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

fn capture_thread(
    shared: Arc<Shared>,
    options: RecordOptions,
    ready: mpsc::Sender<Result<u32, AudioError>>,
) {
    let stream = match open_stream(&shared) {
        Ok((stream, rate)) => {
            let _ = ready.send(Ok(rate));
            stream
        }
        Err(err) => {
            let _ = ready.send(Err(err));
            return;
        }
    };

    let started = Instant::now();
    let mut noise_floor: Option<f32> = None;
    let mut heard_speech = false;
    let mut last_voice = Instant::now();
    while !shared.stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(30));
        let level = f32::from_bits(shared.level.load(Ordering::Relaxed));

        // Learn the room's noise during the first half second.
        if started.elapsed() < Duration::from_millis(500) {
            noise_floor = Some(noise_floor.map_or(level, |n| n.max(level)));
            continue;
        }
        let threshold = (noise_floor.unwrap_or(0.0) * 2.5).max(MIN_SPEECH_LEVEL);
        if level > threshold {
            heard_speech = true;
            shared.heard_speech.store(true, Ordering::Relaxed);
            last_voice = Instant::now();
        }

        let silence_done = options
            .auto_stop_after
            .is_some_and(|after| heard_speech && last_voice.elapsed() >= after);
        let idle_done = options
            .idle_timeout
            .is_some_and(|timeout| !heard_speech && started.elapsed() >= timeout);
        if silence_done || idle_done || started.elapsed() >= options.max_duration {
            shared.finished.store(true, Ordering::Relaxed);
            break;
        }
    }
    drop(stream);
}

fn open_stream(shared: &Arc<Shared>) -> Result<(cpal::Stream, u32), AudioError> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or(AudioError::NoInputDevice)?;
    let supported = device
        .default_input_config()
        .map_err(|e| AudioError::Device(e.to_string()))?;
    let config = supported.config();
    let channels = usize::from(config.channels.max(1));
    let rate = config.sample_rate;

    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, config, channels, shared),
        SampleFormat::I16 => build::<i16>(&device, config, channels, shared),
        SampleFormat::U16 => build::<u16>(&device, config, channels, shared),
        SampleFormat::I32 => build::<i32>(&device, config, channels, shared),
        other => Err(AudioError::Device(format!(
            "unsupported sample format {other:?}"
        ))),
    }?;
    stream
        .play()
        .map_err(|e| AudioError::Device(e.to_string()))?;
    tracing::info!(rate, channels, "microphone opened");
    Ok((stream, rate))
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    shared: &Arc<Shared>,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let shared = Arc::clone(shared);
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _| {
                let mono: Vec<f32> = data
                    .chunks(channels)
                    .map(|frame| {
                        frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / channels as f32
                    })
                    .collect();
                shared.level.store(rms(&mono).to_bits(), Ordering::Relaxed);
                if let Ok(mut samples) = shared.samples.lock() {
                    samples.extend_from_slice(&mono);
                }
            },
            |err| tracing::warn!(%err, "capture stream error"),
            None,
        )
        .map_err(|e| AudioError::Device(e.to_string()))
}
