//! Text-to-speech with the Piper executable.
//!
//! Piper is kept running between sentences: it reads one JSON line per
//! sentence and answers with the path of the WAV it wrote. Loading the voice
//! takes about half a second, synthesis about a tenth, so the tutor starts
//! speaking sooner and pauses less between sentences. If that fails, each
//! sentence runs in its own process as before.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::signal::soften;
use crate::{AudioClip, AudioError};

/// Pause Piper inserts between sentences inside one chunk.
const SENTENCE_SILENCE_SECS: &str = "0.25";

/// Runs Piper for each text chunk, reusing one running process.
#[derive(Debug, Clone)]
pub struct PiperTts {
    exe: PathBuf,
    model: PathBuf,
    sample_rate: u32,
    /// Speech speed: >1.0 is slower, <1.0 is faster.
    length_scale: f32,
    /// `--noise_scale` and `--noise_w`; `None` keeps the voice's defaults.
    variation: Option<(f32, f32)>,
    /// The running process, shared by clones.
    server: Arc<Mutex<Option<PiperServer>>>,
}

/// A Piper process that reads JSON lines.
struct PiperServer {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    dir: tempfile::TempDir,
    next: u64,
}

impl std::fmt::Debug for PiperServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PiperServer")
            .field("pid", &self.child.id())
            .finish_non_exhaustive()
    }
}

impl Drop for PiperServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl PiperServer {
    /// Synthesizes one line of text and returns its 16-bit samples.
    fn speak(&mut self, text: &str) -> Result<Vec<f32>, AudioError> {
        let fail = |e: String| AudioError::Tts(format!("piper: {e}"));
        self.next += 1;
        let path = self.dir.path().join(format!("{}.wav", self.next));
        let line = serde_json::json!({ "text": text, "output_file": path });
        writeln!(self.stdin, "{line}").map_err(|e| fail(e.to_string()))?;
        self.stdin.flush().map_err(|e| fail(e.to_string()))?;
        let mut written = String::new();
        if self
            .stdout
            .read_line(&mut written)
            .map_err(|e| fail(e.to_string()))?
            == 0
        {
            return Err(fail("process ended".into()));
        }
        let mut reader = hound::WavReader::open(&path).map_err(|e| fail(e.to_string()))?;
        let samples = reader
            .samples::<i16>()
            .map(|s| s.map(|v| f32::from(v) / 32768.0))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| fail(e.to_string()));
        drop(reader);
        let _ = std::fs::remove_file(&path);
        samples
    }
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
            variation: None,
            server: Arc::default(),
        })
    }

    /// Starts the Piper process ahead of the first sentence.
    pub fn warm_up(&self) {
        if let Ok(mut server) = self.server.lock()
            && server.is_none()
        {
            match self.start_server() {
                Ok(started) => *server = Some(started),
                Err(error) => tracing::warn!(%error, "could not start piper ahead of time"),
            }
        }
    }

    fn command(&self) -> Result<Command, AudioError> {
        let exe = std::fs::canonicalize(&self.exe).map_err(|e| AudioError::io(&self.exe, e))?;
        let model =
            std::fs::canonicalize(&self.model).map_err(|e| AudioError::io(&self.model, e))?;
        let mut command = Command::new(&exe);
        command
            .arg("--model")
            .arg(model)
            .arg("--length_scale")
            .arg(format!("{:.6}", self.length_scale))
            .arg("--sentence_silence")
            .arg(SENTENCE_SILENCE_SECS);
        if let Some((noise_scale, noise_w)) = self.variation {
            command
                .arg("--noise_scale")
                .arg(format!("{noise_scale:.3}"))
                .arg("--noise_w")
                .arg(format!("{noise_w:.3}"));
        }
        if let Some(dir) = exe.parent() {
            command.current_dir(dir);
        }
        Ok(command)
    }

    fn start_server(&self) -> Result<PiperServer, AudioError> {
        let fail = |e: std::io::Error| AudioError::Tts(format!("could not start piper: {e}"));
        let dir = tempfile::tempdir().map_err(fail)?;
        let mut command = self.command()?;
        command
            .arg("--json-input")
            .arg("--output_dir")
            .arg(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().map_err(fail)?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(AudioError::Tts("piper pipes unavailable".into()));
        };
        Ok(PiperServer {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            dir,
            next: 0,
        })
    }

    /// Synthesizes with the running process, starting it (again) if needed.
    fn synthesize_kept(&self, text: &str) -> Result<Vec<f32>, AudioError> {
        let mut server = self
            .server
            .lock()
            .map_err(|_| AudioError::Tts("piper lock poisoned".into()))?;
        let mut last_error = None;
        for _ in 0..2 {
            if server.is_none() {
                *server = Some(self.start_server()?);
            }
            let Some(running) = server.as_mut() else {
                break;
            };
            match running.speak(text) {
                Ok(samples) => return Ok(samples),
                Err(error) => {
                    *server = None;
                    last_error = Some(error);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| AudioError::Tts("piper unavailable".into())))
    }

    /// Sets intonation (`noise_scale`) and rhythm (`noise_w`) variation,
    /// clamped to ranges that keep speech intelligible.
    pub fn with_variation(mut self, noise_scale: f32, noise_w: f32) -> Self {
        let clamp = |value: f32, range: std::ops::RangeInclusive<f32>, default: f32| {
            if value.is_finite() {
                value.clamp(*range.start(), *range.end())
            } else {
                default
            }
        };
        self.variation = Some((
            clamp(
                noise_scale,
                crate::tts::EXPRESSIVENESS_RANGE,
                crate::tts::DEFAULT_EXPRESSIVENESS,
            ),
            clamp(
                noise_w,
                crate::tts::RHYTHM_VARIATION_RANGE,
                crate::tts::DEFAULT_RHYTHM_VARIATION,
            ),
        ));
        self
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
        if cancelled() {
            return Err(AudioError::Cancelled);
        }
        match self.synthesize_kept(&text) {
            Ok(samples) if !samples.is_empty() => {
                return Ok(AudioClip {
                    samples: soften(&samples, self.sample_rate),
                    sample_rate: self.sample_rate,
                });
            }
            Ok(_) => tracing::warn!("piper returned no audio; running it once instead"),
            Err(error) => tracing::warn!(%error, "kept piper failed; running it once instead"),
        }
        let mut command = self.command()?;
        command.arg("--output_raw");
        let raw = crate::tts::run_child(&mut command, Some(&format!("{text}\n")), cancelled)?;
        if raw.is_empty() || raw.len() % 2 != 0 {
            return Err(AudioError::Tts(
                "piper returned empty or incomplete PCM audio".into(),
            ));
        }
        let samples: Vec<f32> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
            .collect();
        Ok(AudioClip {
            samples: soften(&samples, self.sample_rate),
            sample_rate: self.sample_rate,
        })
    }
}
