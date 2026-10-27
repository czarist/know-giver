//! Text-to-speech with the Piper executable.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{AudioClip, AudioError};

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
            length_scale: length_scale.clamp(0.5, 2.0),
        })
    }

    pub fn synthesize(&self, text: &str) -> Result<AudioClip, AudioError> {
        let text = text.replace(['\r', '\n'], " ");
        let mut command = Command::new(&self.exe);
        command
            .arg("--model")
            .arg(&self.model)
            .arg("--output_raw")
            .arg("--length_scale")
            .arg(format!("{:.2}", self.length_scale))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = self.exe.parent() {
            command.current_dir(dir);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command
            .spawn()
            .map_err(|e| AudioError::Tts(format!("could not start piper: {e}")))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(format!("{text}\n").as_bytes())
                .map_err(|e| AudioError::Tts(e.to_string()))?;
        }
        let mut raw = Vec::new();
        if let Some(mut stdout) = child.stdout.take() {
            stdout
                .read_to_end(&mut raw)
                .map_err(|e| AudioError::Tts(e.to_string()))?;
        }
        let mut stderr = String::new();
        if let Some(mut err) = child.stderr.take() {
            let _ = err.read_to_string(&mut stderr);
        }
        let status = child.wait().map_err(|e| AudioError::Tts(e.to_string()))?;
        if !status.success() {
            return Err(AudioError::Tts(format!(
                "piper exited with {status}: {}",
                stderr.lines().last().unwrap_or_default()
            )));
        }

        let samples = raw
            .chunks_exact(2)
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / f32::from(i16::MAX))
            .collect();
        Ok(AudioClip {
            samples,
            sample_rate: self.sample_rate,
        })
    }
}
