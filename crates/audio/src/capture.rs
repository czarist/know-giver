//! Microphone capture with a simple energy-based end-of-speech detector.
//!
//! When the tutor plays through speakers instead of a headset, the
//! microphone hears it. The detector follows what is being played
//! ([`Recording::follow_playback`]) and learns how much of it comes back, so
//! the tutor's echo does not count as the student speaking.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::signal::rms;
use crate::{AudioClip, AudioError, PlaybackLevel};

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
/// microphones give speech around 0.005 to 0.01, and noise suppression in
/// drivers often makes the room exactly silent, so the bar sits below that;
/// noisy rooms raise it through the noise floor.
const MIN_SPEECH_LEVEL: f32 = 0.0025;
/// Speech must be this many times louder than the room.
const SPEECH_OVER_NOISE: f32 = 3.0;
/// The detector's step.
const TICK: Duration = Duration::from_millis(30);
/// Sound from the speakers keeps arriving after it was played (output
/// latency, the room's reverb): the expected echo fades over about this long.
const ECHO_FADE: Duration = Duration::from_millis(400);
/// Playback this loud is used to learn how much of it the microphone hears.
const ECHO_LEARN_LEVEL: f32 = 0.01;
/// Measurements kept to learn the echo (about 12 seconds of playback).
const ECHO_HISTORY: usize = 400;
/// Measurements needed before trusting what was learned.
const ECHO_MIN_HISTORY: usize = 20;
/// Until then, assume speakers that the microphone hears loudly.
const ECHO_DEFAULT_GAIN: f32 = 1.0;
/// Share of the measured echo levels the estimate covers.
const ECHO_PERCENTILE: f32 = 0.8;
/// The student must be this many times louder than the expected echo.
const ECHO_MARGIN: f32 = 2.0;

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
    /// What the app is playing, so its echo is not taken for speech.
    playback: Mutex<Option<PlaybackLevel>>,
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

    /// Follows what `level` reports being played, so that sound coming back
    /// from the speakers does not count as speech. Cheap to call often; a
    /// new speaker replaces the old one.
    pub fn follow_playback(&self, level: &PlaybackLevel) {
        if let Ok(mut current) = self.shared.playback.lock()
            && !current.as_ref().is_some_and(|c| c.same_as(level))
        {
            *current = Some(level.clone());
        }
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
    let mut echo = EchoGate::default();
    while !shared.stop.load(Ordering::Relaxed) {
        std::thread::sleep(TICK);
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
        let played = shared
            .playback
            .lock()
            .ok()
            .and_then(|p| p.as_ref().map(PlaybackLevel::get))
            .unwrap_or(0.0);
        let threshold = (floor * SPEECH_OVER_NOISE)
            .max(MIN_SPEECH_LEVEL)
            .max(echo.threshold(level, played));
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

/// Tells the tutor's voice coming back through the speakers from the
/// student: learns how loud the microphone hears playback (almost nothing
/// with a headset) and expects that much echo while the tutor is heard.
struct EchoGate {
    /// Recent playback loudness, held while its echo can still arrive.
    envelope: f32,
    /// Microphone level divided by `envelope`, while the tutor plays.
    ratios: VecDeque<f32>,
}

impl Default for EchoGate {
    fn default() -> Self {
        Self {
            envelope: 0.0,
            ratios: VecDeque::with_capacity(ECHO_HISTORY),
        }
    }
}

impl EchoGate {
    /// Takes one detector step (`mic` heard, `played` being played) and
    /// returns the level the microphone must pass to count as speech;
    /// zero when nothing has been played lately.
    fn threshold(&mut self, mic: f32, played: f32) -> f32 {
        let fade = 0.1f32.powf(TICK.as_secs_f32() / ECHO_FADE.as_secs_f32());
        let played = if played.is_finite() { played } else { 0.0 };
        self.envelope = played.max(self.envelope * fade);
        if self.envelope >= ECHO_LEARN_LEVEL && mic.is_finite() {
            if self.ratios.len() == ECHO_HISTORY {
                self.ratios.pop_front();
            }
            self.ratios.push_back(mic / self.envelope);
        }
        self.gain() * self.envelope * ECHO_MARGIN
    }

    /// How loud the echo is next to what is played.
    fn gain(&self) -> f32 {
        if self.ratios.len() < ECHO_MIN_HISTORY {
            return ECHO_DEFAULT_GAIN;
        }
        let mut sorted: Vec<f32> = self.ratios.iter().copied().collect();
        sorted.sort_by(f32::total_cmp);
        let at = ((sorted.len() - 1) as f32 * ECHO_PERCENTILE) as usize;
        sorted.get(at).copied().unwrap_or(ECHO_DEFAULT_GAIN)
    }
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
    use super::{EchoGate, median};

    /// Runs `steps` detector steps and returns the last threshold.
    fn run(gate: &mut EchoGate, steps: usize, mic: f32, played: f32) -> f32 {
        (0..steps).fold(0.0, |_, _| gate.threshold(mic, played))
    }

    #[test]
    fn speakers_echo_is_not_speech_but_the_student_over_it_is() {
        let mut gate = EchoGate::default();
        // The tutor plays at 0.1 and the microphone hears 0.03 of it.
        let threshold = run(&mut gate, 100, 0.03, 0.1);
        assert!(threshold > 0.03, "echo passed: {threshold}");
        // The student speaks up close, well over the echo.
        assert!(0.1 > threshold, "student blocked: {threshold}");
    }

    #[test]
    fn echo_fades_after_playback_stops() {
        let mut gate = EchoGate::default();
        run(&mut gate, 100, 0.03, 0.1);
        // Right after the tutor stops, its last words still come back.
        assert!(gate.threshold(0.03, 0.0) > 0.03);
        // Half a second later the room is the student's again.
        assert!(run(&mut gate, 20, 0.0, 0.0) < 0.002);
    }

    #[test]
    fn a_headset_leaves_the_microphone_open() {
        let mut gate = EchoGate::default();
        // The microphone hears only the room while the tutor plays.
        let threshold = run(&mut gate, 100, 0.001, 0.1);
        assert!(threshold < 0.0025, "headset blocked: {threshold}");
        assert_eq!(EchoGate::default().threshold(0.01, 0.0), 0.0);
    }

    #[test]
    fn noise_floor_ignores_an_early_answer() {
        // Quiet room, then the student starts talking within the first half second.
        let mut levels = vec![0.002, 0.003, 0.002, 0.004, 0.09, 0.12, 0.1];
        assert_eq!(median(&mut levels), 0.004);
        assert_eq!(median(&mut []), 0.0);
    }
}
