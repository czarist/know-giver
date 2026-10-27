//! Speech playback on the default output device.

use std::num::NonZero;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use crate::{AudioClip, AudioError};

enum Command {
    Play(AudioClip),
    Stop,
}

/// Plays clips one after another. The output device lives on its own thread;
/// this handle is cheap to clone and safe to share.
#[derive(Debug, Clone)]
pub struct Speaker {
    tx: mpsc::Sender<Command>,
    playing: Arc<AtomicBool>,
}

impl Speaker {
    /// Opens the default output device.
    pub fn open() -> Result<Self, AudioError> {
        let (tx, rx) = mpsc::channel::<Command>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), AudioError>>();
        let playing = Arc::new(AtomicBool::new(false));
        let thread_playing = Arc::clone(&playing);
        std::thread::Builder::new()
            .name("kg-playback".into())
            .spawn(move || playback_thread(rx, ready_tx, thread_playing))
            .map_err(|e| AudioError::Device(e.to_string()))?;
        ready_rx
            .recv()
            .map_err(|_| AudioError::NoOutputDevice("playback thread ended early".into()))??;
        Ok(Self { tx, playing })
    }

    /// Queues a clip after whatever is already playing.
    pub fn play(&self, clip: AudioClip) {
        if clip.samples.is_empty() {
            return;
        }
        self.playing.store(true, Ordering::Relaxed);
        let _ = self.tx.send(Command::Play(clip));
    }

    /// Stops playback and drops anything queued.
    pub fn stop(&self) {
        let _ = self.tx.send(Command::Stop);
        self.playing.store(false, Ordering::Relaxed);
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }
}

fn playback_thread(
    rx: mpsc::Receiver<Command>,
    ready: mpsc::Sender<Result<(), AudioError>>,
    playing: Arc<AtomicBool>,
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
            Ok(Command::Play(clip)) => {
                let (Some(channels), Some(rate)) =
                    (NonZero::new(1u16), NonZero::new(clip.sample_rate))
                else {
                    continue;
                };
                player.append(rodio::buffer::SamplesBuffer::new(
                    channels,
                    rate,
                    clip.samples,
                ));
                player.play();
            }
            Ok(Command::Stop) => {
                player.clear();
                player.play();
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if player.empty() {
            playing.store(false, Ordering::Relaxed);
        }
    }
}
