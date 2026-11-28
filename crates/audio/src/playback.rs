//! Speech playback on the chosen output device (see [`crate::devices`]).

use std::collections::VecDeque;
use std::num::NonZero;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::Source;

use crate::{AudioClip, AudioError};

/// Called once, when a clip's first sample reaches the output device.
pub type OnStart = Box<dyn FnOnce() + Send>;

enum Command {
    Play(AudioClip, Option<OnStart>),
    Stop,
    Pause,
    Resume,
}

/// Loudness of what a [`Speaker`] is playing right now, readable from any
/// thread. The microphone uses it to tell the tutor's echo from the student.
#[derive(Debug, Clone, Default)]
pub struct PlaybackLevel(Arc<AtomicU32>);

impl PlaybackLevel {
    /// Roughly `0.0..=1.0`; zero when nothing plays.
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    /// True when both handles follow the same speaker.
    pub fn same_as(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Plays clips one after another. The output device lives on its own thread;
/// this handle is cheap to clone and safe to share.
#[derive(Debug, Clone)]
pub struct Speaker {
    tx: mpsc::Sender<Command>,
    playing: Arc<AtomicBool>,
    /// Held by [`Speaker::pause`]: the queue waits, nothing is dropped.
    paused: Arc<AtomicBool>,
    /// Loudness of what is playing right now (f32 bits), for lip-sync.
    level: Arc<AtomicU32>,
}

impl Speaker {
    /// Opens the chosen output device, following later changes of it.
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
        Ok(Self {
            tx,
            playing,
            paused: Arc::new(AtomicBool::new(false)),
            level,
        })
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

    /// Stops playback and drops anything queued. Also ends a pause.
    pub fn stop(&self) {
        let _ = self.tx.send(Command::Stop);
        self.playing.store(false, Ordering::Relaxed);
        self.paused.store(false, Ordering::Relaxed);
        self.level.store(0, Ordering::Relaxed);
    }

    /// Holds playback where it is; clips queued meanwhile wait too.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
        self.level.store(0, Ordering::Relaxed);
        let _ = self.tx.send(Command::Pause);
    }

    /// Goes on from where [`Speaker::pause`] held it.
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
        let _ = self.tx.send(Command::Resume);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    /// Loudness of the audio being played, roughly `0.0..=1.0`.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// A handle that follows [`Speaker::level`] from other threads.
    pub fn playback_level(&self) -> PlaybackLevel {
        PlaybackLevel(Arc::clone(&self.level))
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

/// A clip's start callback, shared so a clip replayed on a new device does
/// not call it twice.
type StartSlot = Arc<Mutex<Option<OnStart>>>;

/// Wraps a source and calls a function when its first sample is pulled.
struct Announced<S> {
    inner: S,
    on_start: StartSlot,
    announced: bool,
}

impl<S> Announced<S> {
    fn new(inner: S, on_start: StartSlot) -> Self {
        Self {
            inner,
            on_start,
            announced: false,
        }
    }
}

impl<S: Source> Iterator for Announced<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.announced {
            self.announced = true;
            if let Some(on_start) = self.on_start.lock().ok().and_then(|mut s| s.take()) {
                on_start();
            }
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

/// A clip handed to the device and not finished yet, kept to replay it when
/// the output changes.
struct Queued {
    clip: AudioClip,
    on_start: StartSlot,
}

/// The open output device and the player feeding it.
struct Output {
    _sink: rodio::MixerDeviceSink,
    player: rodio::Player,
}

fn open_output() -> Result<Output, AudioError> {
    let mut sink = crate::devices::open_output()?;
    sink.log_on_drop(false);
    let player = rodio::Player::connect_new(sink.mixer());
    Ok(Output {
        _sink: sink,
        player,
    })
}

fn append(player: &rodio::Player, queued: &Queued, level: &Arc<AtomicU32>) {
    let (Some(channels), Some(rate)) = (NonZero::new(1u16), NonZero::new(queued.clip.sample_rate))
    else {
        return;
    };
    let source = rodio::buffer::SamplesBuffer::new(channels, rate, queued.clip.samples.clone());
    let source = Announced::new(source, Arc::clone(&queued.on_start));
    player.append(Metered::new(source, Arc::clone(level)));
}

fn playback_thread(
    rx: mpsc::Receiver<Command>,
    ready: mpsc::Sender<Result<(), AudioError>>,
    playing: Arc<AtomicBool>,
    level: Arc<AtomicU32>,
) {
    let mut generation = crate::devices::output_generation();
    let mut output = match open_output() {
        Ok(output) => output,
        Err(err) => {
            let _ = ready.send(Err(err));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    let mut queue: VecDeque<Queued> = VecDeque::new();
    let mut paused = false;

    loop {
        match rx.recv_timeout(Duration::from_millis(40)) {
            Ok(Command::Play(clip, on_start)) => {
                let queued = Queued {
                    clip,
                    on_start: Arc::new(Mutex::new(on_start)),
                };
                append(&output.player, &queued, &level);
                queue.push_back(queued);
                if !paused {
                    output.player.play();
                }
            }
            Ok(Command::Stop) => {
                output.player.clear();
                output.player.play();
                queue.clear();
                paused = false;
                level.store(0, Ordering::Relaxed);
            }
            Ok(Command::Pause) => {
                output.player.pause();
                paused = true;
                level.store(0, Ordering::Relaxed);
            }
            Ok(Command::Resume) => {
                output.player.play();
                paused = false;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        // Clips the device finished are gone from the player's queue.
        while queue.len() > output.player.len() {
            queue.pop_front();
        }
        let now = crate::devices::output_generation();
        if now != generation {
            generation = now;
            switch_output(&mut output, &queue, paused, &level);
        }
        if output.player.empty() {
            playing.store(false, Ordering::Relaxed);
            level.store(0, Ordering::Relaxed);
        }
    }
}

/// Moves playback to the newly chosen device. The sentence being spoken
/// starts again from its beginning there; the rest of the queue follows.
fn switch_output(
    output: &mut Output,
    queue: &VecDeque<Queued>,
    paused: bool,
    level: &Arc<AtomicU32>,
) {
    let next = match open_output() {
        Ok(next) => next,
        Err(err) => {
            tracing::warn!(%err, "could not switch the output device");
            return;
        }
    };
    output.player.clear();
    level.store(0, Ordering::Relaxed);
    for queued in queue {
        append(&next.player, queued, level);
    }
    if paused {
        next.player.pause();
    } else {
        next.player.play();
    }
    *output = next;
    tracing::info!("output device switched");
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
        let on_start: OnStart = Box::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        let slot: StartSlot = Arc::new(Mutex::new(Some(on_start)));
        let samples = || rodio::buffer::SamplesBuffer::new(channels, rate, vec![0.1f32; 10]);
        let mut source = Announced::new(samples(), Arc::clone(&slot));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        while source.next().is_some() {}
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        // Replayed on another device: heard again, announced only once.
        let mut replay = Announced::new(samples(), slot);
        while replay.next().is_some() {}
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}
