//! Speech playback on the default output device.

use std::num::NonZero;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use rodio::Source;

use crate::{AudioClip, AudioError};

/// Called once, when a clip's first sample reaches the output device.
pub type OnStart = Box<dyn FnOnce() + Send>;

enum Command {
    Play(AudioClip, Option<OnStart>),
    Stop,
}

/// Plays clips one after another. The output device lives on its own thread;
/// this handle is cheap to clone and safe to share.
#[derive(Debug, Clone)]
pub struct Speaker {
    tx: mpsc::Sender<Command>,
    playing: Arc<AtomicBool>,
    /// Loudness of what is playing right now (f32 bits), for lip-sync.
    level: Arc<AtomicU32>,
}

impl Speaker {
    /// Opens the default output device.
    pub fn open() -> Result<Self, AudioError> {
        let (tx, rx) = mpsc::channel::<Command>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), AudioError>>();
        let playing = Arc::new(AtomicBool::new(false));
        let level = Arc::new(AtomicU32::new(0));
        let thread_playing = Arc::clone(&playing);
        let thread_level = Arc::clone(&level);
        std::thread::Builder::new()
            .name("kg-playback".into())
            .spawn(move || playback_thread(rx, ready_tx, thread_playing, thread_level))
            .map_err(|e| AudioError::Device(e.to_string()))?;
        ready_rx
            .recv()
            .map_err(|_| AudioError::NoOutputDevice("playback thread ended early".into()))??;
        Ok(Self { tx, playing, level })
    }

    /// Queues a clip after whatever is already playing.
    pub fn play(&self, clip: AudioClip) {
        self.queue(clip, None);
    }

    /// Queues a clip and calls `on_start` when it begins to be heard, so
    /// captions can follow the voice. A clip dropped by `stop` never calls it.
    pub fn play_then(&self, clip: AudioClip, on_start: OnStart) {
        self.queue(clip, Some(on_start));
    }

    fn queue(&self, clip: AudioClip, on_start: Option<OnStart>) {
        if clip.samples.is_empty() {
            return;
        }
        self.playing.store(true, Ordering::Relaxed);
        let _ = self.tx.send(Command::Play(clip, on_start));
    }

    /// Stops playback and drops anything queued.
    pub fn stop(&self) {
        let _ = self.tx.send(Command::Stop);
        self.playing.store(false, Ordering::Relaxed);
        self.level.store(0, Ordering::Relaxed);
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    /// Loudness of the audio being played, roughly `0.0..=1.0`.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
}

/// Wraps a source and publishes its loudness every few milliseconds, as the
/// device pulls samples, so animations follow what is actually heard.
struct Metered<S> {
    inner: S,
    level: Arc<AtomicU32>,
    sum: f32,
    count: usize,
    window: usize,
}

impl<S: Source> Metered<S> {
    fn new(inner: S, level: Arc<AtomicU32>) -> Self {
        // About 25 ms of audio per measurement.
        let window = (inner.sample_rate().get() as usize / 40).max(1);
        Self {
            inner,
            level,
            sum: 0.0,
            count: 0,
            window,
        }
    }
}

/// Wraps a source and calls a function when its first sample is pulled.
struct Announced<S> {
    inner: S,
    on_start: Option<OnStart>,
}

impl<S: Source> Iterator for Announced<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(on_start) = self.on_start.take() {
            on_start();
        }
        self.inner.next()
    }
}

impl<S: Source> Source for Announced<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

impl<S: Source> Iterator for Metered<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next();
        match sample {
            Some(value) => {
                self.sum += value * value;
                self.count += 1;
                if self.count >= self.window {
                    let rms = (self.sum / self.count as f32).sqrt();
                    self.level.store(rms.to_bits(), Ordering::Relaxed);
                    self.sum = 0.0;
                    self.count = 0;
                }
            }
            None => self.level.store(0, Ordering::Relaxed),
        }
        sample
    }
}

impl<S: Source> Source for Metered<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

fn playback_thread(
    rx: mpsc::Receiver<Command>,
    ready: mpsc::Sender<Result<(), AudioError>>,
    playing: Arc<AtomicBool>,
    level: Arc<AtomicU32>,
) {
    let mut sink = match rodio::DeviceSinkBuilder::open_default_sink() {
        Ok(sink) => sink,
        Err(err) => {
            let _ = ready.send(Err(AudioError::NoOutputDevice(err.to_string())));
            return;
        }
    };
    sink.log_on_drop(false);
    let player = rodio::Player::connect_new(sink.mixer());
    let _ = ready.send(Ok(()));

    loop {
        match rx.recv_timeout(Duration::from_millis(40)) {
            Ok(Command::Play(clip, on_start)) => {
                let (Some(channels), Some(rate)) =
                    (NonZero::new(1u16), NonZero::new(clip.sample_rate))
                else {
                    continue;
                };
                let source = rodio::buffer::SamplesBuffer::new(channels, rate, clip.samples);
                let source = Announced {
                    inner: source,
                    on_start,
                };
                player.append(Metered::new(source, Arc::clone(&level)));
                player.play();
            }
            Ok(Command::Stop) => {
                player.clear();
                player.play();
                level.store(0, Ordering::Relaxed);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if player.empty() {
            playing.store(false, Ordering::Relaxed);
            level.store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metered_source_reports_loudness() {
        let rate = NonZero::new(16_000u32).unwrap();
        let channels = NonZero::new(1u16).unwrap();
        let level = Arc::new(AtomicU32::new(0));
        let source = rodio::buffer::SamplesBuffer::new(channels, rate, vec![0.5f32; 1_000]);
        let mut metered = Metered::new(source, Arc::clone(&level));
        for _ in 0..400 {
            metered.next();
        }
        let measured = f32::from_bits(level.load(Ordering::Relaxed));
        assert!((measured - 0.5).abs() < 1e-3);
        while metered.next().is_some() {}
        assert_eq!(f32::from_bits(level.load(Ordering::Relaxed)), 0.0);
    }

    #[test]
    fn announced_source_calls_back_once_on_the_first_sample() {
        let rate = NonZero::new(16_000u32).unwrap();
        let channels = NonZero::new(1u16).unwrap();
        let calls = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&calls);
        let mut source = Announced {
            inner: rodio::buffer::SamplesBuffer::new(channels, rate, vec![0.1f32; 10]),
            on_start: Some(Box::new(move || {
                counter.fetch_add(1, Ordering::Relaxed);
            })),
        };
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        while source.next().is_some() {}
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}
