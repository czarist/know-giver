//! Text-to-speech with the Piper executable.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::signal::soften;
use crate::{AudioClip, AudioError};

/// Pause Piper inserts between sentences inside one chunk.
const SENTENCE_SILENCE_SECS: &str = "0.25";

/// Runs `piper --output_raw` once per text chunk.
#[derive(Debug, Clone)]
pub struct PiperTts {
    exe: PathBuf,
    model: PathBuf,
    sample_rate: u32,
    /// Speech speed: >1.0 is slower, <1.0 is faster.
    length_scale: f32,
}

impl PiperTts {
    /// `voice_config` is the `.onnx.json` next to the voice model.
    pub fn new(
        exe: PathBuf,
        model: PathBuf,
        voice_config: &Path,
        length_scale: f32,
    ) -> Result<Self, AudioError> {
        let raw =
            std::fs::read_to_string(voice_config).map_err(|e| AudioError::io(voice_config, e))?;
        let json: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| AudioError::Tts(format!("bad voice config: {e}")))?;
        let sample_rate = json["audio"]["sample_rate"]
            .as_u64()
            .and_then(|r| u32::try_from(r).ok())
            .unwrap_or(22_050);
        Ok(Self {
            exe,
            model,
            sample_rate,
            length_scale: length_scale.clamp(0.25, 4.0),
        })
    }

    pub fn synthesize(&self, text: &str) -> Result<AudioClip, AudioError> {
        self.synthesize_cancellable(text, &|| false)
    }

    pub(crate) fn synthesize_cancellable(
        &self,
        text: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<AudioClip, AudioError> {
        let text = text.replace(['\r', '\n'], " ");
        let exe = std::fs::canonicalize(&self.exe).map_err(|e| AudioError::io(&self.exe, e))?;
        let model =
            std::fs::canonicalize(&self.model).map_err(|e| AudioError::io(&self.model, e))?;
        let mut command = Command::new(&exe);
        command
            .arg("--model")
            .arg(model)
            .arg("--output_raw")
            .arg("--length_scale")
            .arg(format!("{:.6}", self.length_scale))
            .arg("--sentence_silence")
            .arg(SENTENCE_SILENCE_SECS);
        if let Some(dir) = exe.parent() {
            command.current_dir(dir);
        }
        let raw = crate::tts::run_child(&mut command, Some(&format!("{text}\n")), cancelled)?;
        if raw.is_empty() || raw.len() % 2 != 0 {
            return Err(AudioError::Tts(
                "piper returned empty or incomplete PCM audio".into(),
            ));
        }
        let samples: Vec<f32> = raw
            .chunks_exact(2)
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
            .collect();
        Ok(AudioClip {
            samples: soften(&samples, self.sample_rate),
            sample_rate: self.sample_rate,
        })
    }
}
