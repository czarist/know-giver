//! Microphone capture with a simple energy-based end-of-speech detector.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
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
    /// Room noise level measured by an earlier recording. Without it, the
    /// first half second is used to measure it.
    pub noise_floor: Option<f32>,
}

impl Default for RecordOptions {
    fn default() -> Self {
        Self {
            auto_stop_after: Some(Duration::from_millis(1_400)),
            idle_timeout: None,
            max_duration: Duration::from_secs(90),
            noise_floor: None,
        }
    }
}

/// Minimum level counted as speech, whatever the noise floor. Quiet
/// microphones give speech around 0.01, so the bar is set below that.
const MIN_SPEECH_LEVEL: f32 = 0.005;
/// Speech must be this many times louder than the room.
const SPEECH_OVER_NOISE: f32 = 3.0;

/// The audio kept so far. Positions count from the start of the recording,
/// also after older audio was let go with [`Recording::forget_before`].
#[derive(Default)]
struct Buffer {
    /// Samples let go from the front.
    base: usize,
    data: Vec<f32>,
}

impl Buffer {
    fn len(&self) -> usize {
        self.base + self.data.len()
    }
}

#[derive(Default)]
struct Shared {
    samples: Mutex<Buffer>,
    /// Latest block level (f32 bits), for the UI meter.
    level: AtomicU32,
    stop: AtomicBool,
    /// Set when the recorder stopped by itself (silence or time limit).
    finished: AtomicBool,
    heard_speech: AtomicBool,
    /// Samples recorded when speech was last heard.
    voice_end: AtomicUsize,
    /// Milliseconds since the start when speech was last heard.
    voice_at_ms: AtomicU64,
    /// Milliseconds since the start, updated by the detector.
    now_ms: AtomicU64,
    /// Room noise level (f32 bits), once known.
    noise_floor: AtomicU32,
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

    /// How long the student has been quiet after speaking; `None` before
    /// any speech.
    pub fn quiet_for(&self) -> Option<Duration> {
        if !self.heard_speech() {
            return None;
        }
        let now = self.shared.now_ms.load(Ordering::Relaxed);
        let voice = self.shared.voice_at_ms.load(Ordering::Relaxed);
        Some(Duration::from_millis(now.saturating_sub(voice)))
    }

    /// Room noise level, once measured.
    pub fn noise_floor(&self) -> Option<f32> {
        let floor = f32::from_bits(self.shared.noise_floor.load(Ordering::Relaxed));
        (floor > 0.0).then_some(floor)
    }

    /// Samples recorded up to the last moment speech was heard.
    pub fn voice_end(&self) -> usize {
        self.shared.voice_end.load(Ordering::Relaxed)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Samples recorded so far.
    pub fn len(&self) -> usize {
        self.shared.samples.lock().map_or(0, |s| s.len())
    }

    /// Lets go of the audio before `position`, to keep a long-running
    /// recording small. Positions stay the same.
    pub fn forget_before(&self, position: usize) {
        if let Ok(mut buffer) = self.shared.samples.lock() {
            let drop = position.saturating_sub(buffer.base).min(buffer.data.len());
            buffer.data.drain(..drop);
            buffer.base += drop;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A copy of the audio so far, for live captions while still recording.
    pub fn snapshot(&self) -> AudioClip {
        self.snapshot_from(0)
    }

    /// A copy of the audio from sample `start` on.
    pub fn snapshot_from(&self, start: usize) -> AudioClip {
        AudioClip {
            samples: self
                .shared
                .samples
                .lock()
                .map(|s| {
                    let from = start.saturating_sub(s.base);
                    s.data.get(from..).unwrap_or_default().to_vec()
                })
                .unwrap_or_default(),
            sample_rate: self.sample_rate,
        }
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
            .map(|mut s| std::mem::take(&mut s.data))
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
    let mut learning = Vec::new();
    let mut noise_floor = options.noise_floor.filter(|f| f.is_finite() && *f > 0.0);
    if let Some(floor) = noise_floor {
        shared.noise_floor.store(floor.to_bits(), Ordering::Relaxed);
    }
    let mut heard_speech = false;
    let mut last_voice = Instant::now();
    while !shared.stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(30));
        let level = f32::from_bits(shared.level.load(Ordering::Relaxed));
        shared
            .now_ms
            .store(started.elapsed().as_millis() as u64, Ordering::Relaxed);

        // Learn the room's noise during the first half second. The median
        // keeps a student who answers right away from counting as noise.
        if noise_floor.is_none() && started.elapsed() < Duration::from_millis(500) {
            learning.push(level);
            continue;
        }
        let floor = *noise_floor.get_or_insert_with(|| {
            // Never zero, so it reads as "measured".
            let floor = median(&mut learning).max(1e-6);
            shared.noise_floor.store(floor.to_bits(), Ordering::Relaxed);
            floor
        });
        let threshold = (floor * SPEECH_OVER_NOISE).max(MIN_SPEECH_LEVEL);
        if level > threshold {
            heard_speech = true;
            shared.heard_speech.store(true, Ordering::Relaxed);
            last_voice = Instant::now();
            shared
                .voice_at_ms
                .store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
            let recorded = shared.samples.lock().map_or(0, |s| s.len());
            shared.voice_end.store(recorded, Ordering::Relaxed);
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

fn median(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
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
                    samples.data.extend_from_slice(&mono);
                }
            },
            |err| tracing::warn!(%err, "capture stream error"),
            None,
        )
        .map_err(|e| AudioError::Device(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::median;

    #[test]
    fn noise_floor_ignores_an_early_answer() {
        // Quiet room, then the student starts talking within the first half second.
        let mut levels = vec![0.002, 0.003, 0.002, 0.004, 0.09, 0.12, 0.1];
        assert_eq!(median(&mut levels), 0.004);
        assert_eq!(median(&mut []), 0.0);
    }
}
