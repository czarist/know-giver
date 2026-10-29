//! Fast speech recognition with NVIDIA Parakeet TDT 0.6B v3, which covers
//! Portuguese among 25 languages. The pinned sherpa-onnx runtime runs it as a
//! local server that keeps the model loaded, so one utterance takes about a
//! third of a second on CPU, against several seconds for Whisper small.
//! Nothing leaves the computer: the server only listens on 127.0.0.1.

use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tungstenite::{Message, WebSocket};

use crate::signal::{prepare_for_stt, resample};
use crate::{AudioClip, AudioError, STT_SAMPLE_RATE};

/// Loading the model takes a few seconds; give slow disks some room.
const START_TIMEOUT: Duration = Duration::from_secs(60);
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
/// Our own copy of the server executable on Windows, so a server left
/// behind by a crash can be recognized and stopped by name.
#[cfg(windows)]
const SERVER_COPY: &str = "know-giver-stt.exe";

/// A running Parakeet server and a connection to it.
pub struct ParakeetStt {
    child: Child,
    socket: WebSocket<TcpStream>,
}

impl std::fmt::Debug for ParakeetStt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParakeetStt")
            .field("pid", &self.child.id())
            .finish_non_exhaustive()
    }
}

impl ParakeetStt {
    /// Starts the server from `exe` (sherpa-onnx-offline-websocket-server)
    /// with the model in `model_dir`, and waits until it accepts a connection.
    pub fn start(exe: &Path, model_dir: &Path) -> Result<Self, AudioError> {
        let started = Instant::now();
        let exe = server_executable(exe)?;
        let model = |name: &str| -> Result<String, AudioError> {
            let path = model_dir.join(name);
            if !path.is_file() {
                return Err(AudioError::ModelMissing(path.display().to_string()));
            }
            Ok(crate::tts::plain_path(path).display().to_string())
        };
        let port = free_port()?;
        let threads = std::thread::available_parallelism()
            .map_or(4, |n| n.get() / 2)
            .clamp(2, 4);
        let log = std::env::temp_dir().join("know-giver-stt.log");

        let mut command = Command::new(&exe);
        command
            .arg(format!("--port={port}"))
            .arg("--num-io-threads=1")
            .arg("--num-work-threads=1")
            .arg("--max-batch-size=1")
            .arg(format!("--num-threads={threads}"))
            .arg(format!("--log-file={}", log.display()))
            .arg(format!("--encoder={}", model("encoder.int8.onnx")?))
            .arg(format!("--decoder={}", model("decoder.int8.onnx")?))
            .arg(format!("--joiner={}", model("joiner.int8.onnx")?))
            .arg(format!("--tokens={}", model("tokens.txt")?))
            .arg("--model-type=nemo_transducer")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(dir) = exe.parent() {
            command.current_dir(dir);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command
            .spawn()
            .map_err(|e| AudioError::Model(format!("could not start the speech server: {e}")))?;

        let url = format!("ws://127.0.0.1:{port}");
        let socket = loop {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(AudioError::Model(format!(
                    "speech server stopped while loading ({status}); see {}",
                    log.display()
                )));
            }
            if started.elapsed() > START_TIMEOUT {
                let _ = child.kill();
                return Err(AudioError::Model("speech server did not start".into()));
            }
            if let Ok(stream) = TcpStream::connect(("127.0.0.1", port))
                && let Ok((socket, _)) = tungstenite::client(url.as_str(), stream)
            {
                break socket;
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        socket
            .get_ref()
            .set_read_timeout(Some(REPLY_TIMEOUT))
            .map_err(|e| AudioError::Model(e.to_string()))?;
        tracing::info!(
            port,
            threads,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "parakeet ready"
        );
        Ok(Self { child, socket })
    }

    /// True while the server process is alive.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Transcribes a clip. Returns an empty string for silence.
    pub fn transcribe(&mut self, clip: &AudioClip) -> Result<String, AudioError> {
        let pcm = prepare_for_stt(
            &resample(&clip.samples, clip.sample_rate, STT_SAMPLE_RATE),
            STT_SAMPLE_RATE,
        );
        if pcm.len() < STT_SAMPLE_RATE as usize / 2 {
            return Ok(String::new());
        }
        let started = Instant::now();
        let fail = |e: tungstenite::Error| AudioError::Model(format!("speech server: {e}"));
        self.socket
            .send(Message::Binary(request(&pcm)?.into()))
            .map_err(fail)?;
        let reply = loop {
            match self.socket.read().map_err(fail)? {
                Message::Text(text) => break text.to_string(),
                Message::Close(_) => {
                    return Err(AudioError::Model("speech server closed".into()));
                }
                _ => {}
            }
        };
        let text = parse_reply(&reply)?;
        tracing::info!(
            audio_secs = clip.duration_secs(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            chars = text.len(),
            "transcribed with parakeet"
        );
        Ok(text)
    }
}

impl Drop for ParakeetStt {
    fn drop(&mut self) {
        let _ = self.socket.send(Message::Text("Done".into()));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One utterance in the server's format: sample rate and byte count as
/// little-endian `i32`, then the samples as little-endian `f32`.
fn request(pcm: &[f32]) -> Result<Vec<u8>, AudioError> {
    let bytes = i32::try_from(pcm.len() * 4)
        .map_err(|_| AudioError::Model("recording too long for the speech server".into()))?;
    let mut out = Vec::with_capacity(8 + pcm.len() * 4);
    out.extend_from_slice(&(STT_SAMPLE_RATE as i32).to_le_bytes());
    out.extend_from_slice(&bytes.to_le_bytes());
    for sample in pcm {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(out)
}

fn parse_reply(reply: &str) -> Result<String, AudioError> {
    #[derive(serde::Deserialize)]
    struct Reply {
        text: String,
    }
    let parsed: Reply = serde_json::from_str(reply)
        .map_err(|e| AudioError::Model(format!("unexpected speech server reply: {e}")))?;
    let text = parsed.text.trim();
    // A lone punctuation mark is what the model gives for noise.
    if text.chars().all(|c| !c.is_alphanumeric()) {
        return Ok(String::new());
    }
    Ok(text.to_owned())
}

fn free_port() -> Result<u16, AudioError> {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| AudioError::Model(format!("no free local port: {e}")))
}

/// On Windows, a renamed copy next to the original (it needs the DLLs
/// there), after stopping any copy a crash left running.
fn server_executable(exe: &Path) -> Result<PathBuf, AudioError> {
    #[cfg(windows)]
    {
        let copy = exe.with_file_name(SERVER_COPY);
        let mut kill = Command::new("taskkill");
        {
            use std::os::windows::process::CommandExt;
            kill.creation_flags(0x0800_0000);
        }
        let _ = kill
            .args(["/F", "/IM", SERVER_COPY])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let same = |a: &Path, b: &Path| {
            std::fs::metadata(a)
                .ok()
                .zip(std::fs::metadata(b).ok())
                .is_some_and(|(a, b)| a.len() == b.len())
        };
        if !same(exe, &copy) {
            std::fs::copy(exe, &copy).map_err(|e| AudioError::io(&copy, e))?;
        }
        Ok(copy)
    }
    #[cfg(not(windows))]
    {
        Ok(exe.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_has_header_then_samples() {
        let out = request(&[0.5, -1.0]).unwrap();
        assert_eq!(&out[..4], &16_000i32.to_le_bytes());
        assert_eq!(&out[4..8], &8i32.to_le_bytes());
        assert_eq!(&out[8..12], &0.5f32.to_le_bytes());
        assert_eq!(out.len(), 16);
    }

    #[test]
    fn reply_text_is_trimmed_and_noise_dropped() {
        let reply = r#"{"lang": "", "text": " Sim, entendi.", "timestamps": [0.9]}"#;
        assert_eq!(parse_reply(reply).unwrap(), "Sim, entendi.");
        assert_eq!(parse_reply(r#"{"text": " ."}"#).unwrap(), "");
        assert!(parse_reply("not json").is_err());
    }
}
