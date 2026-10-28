//! Audio layer: microphone capture, speech-to-text, text-to-speech, playback
//! and local model management.
//!
//! - Capture and playback: cpal / rodio (pure Rust on Windows, macOS, Linux).
//! - STT: Whisper running on candle (pure Rust, CPU), weights from Hugging Face.
//! - TTS: Piper or Kokoro as a child process, plus an optional HTTP speech service.
//!   No C/C++ code is compiled into the app.

mod capture;
mod kokoro_runtime;
mod models;
mod piper;
mod playback;
mod signal;
mod speech_text;
mod tts;
mod voice;
mod whisper;

pub use capture::{RecordOptions, Recording};
pub use kokoro_runtime::prepare_kokoro_executable;
pub use models::{ModelId, ModelStore, PiperVoice, WhisperModel};
pub use piper::PiperTts;
pub use playback::Speaker;
pub use signal::{resample, soften, split_for_speech};
pub use speech_text::normalize_for_speech;
pub use tts::{
    DEFAULT_EXPRESSIVENESS, DEFAULT_RHYTHM_VARIATION, EXPRESSIVENESS_RANGE, RHYTHM_VARIATION_RANGE,
    SpeechOptions, Synthesizer,
};
pub use voice::{CloudSpeechConfig, KokoroVoice, TtsBackend, Voice, VoiceGender};
pub use whisper::WhisperStt;

use std::path::PathBuf;

/// Sample rate Whisper expects.
pub const STT_SAMPLE_RATE: u32 = 16_000;

/// Mono PCM audio at a known sample rate.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioClip {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl AudioClip {
    pub fn duration_secs(&self) -> f32 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.samples.len() as f32 / self.sample_rate as f32
        }
    }
}

/// Errors from audio devices, speech models and model downloads.
///
/// Messages are English and meant for logs; the UI shows translated text.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no input device found")]
    NoInputDevice,
    #[error("no output device available: {0}")]
    NoOutputDevice(String),
    #[error("audio device failure: {0}")]
    Device(String),
    #[error("model not installed: {0}")]
    ModelMissing(String),
    #[error("speech model failure: {0}")]
    Model(String),
    #[error("text-to-speech failure: {0}")]
    Tts(String),
    #[error("download of {url} failed: {reason}")]
    Download { url: String, reason: String },
    #[error("download cancelled")]
    Cancelled,
    #[error("i/o error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("not supported on this platform: {0}")]
    Unsupported(String),
}

impl AudioError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
