//! Cancellable local and opt-in HTTP speech synthesis.

use std::ffi::OsString;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::{AudioClip, AudioError, CloudSpeechConfig, KokoroVoice, ModelStore, PiperTts, Voice};

/// Voice controls common to local and remote engines.
#[derive(Debug, Clone, Copy)]
pub struct SpeechOptions {
    pub rate: f32,
    /// Semitones; synthesis speed is compensated before shifting playback pitch.
    pub pitch: f32,
    /// Piper intonation variation (`--noise_scale`); higher is livelier.
    pub expressiveness: f32,
    /// Piper rhythm variation (`--noise_w`); higher is less regular.
    pub rhythm_variation: f32,
}

/// Piper's usual defaults for medium voices.
pub const DEFAULT_EXPRESSIVENESS: f32 = 0.667;
pub const DEFAULT_RHYTHM_VARIATION: f32 = 0.8;
pub const EXPRESSIVENESS_RANGE: std::ops::RangeInclusive<f32> = 0.3..=1.0;
pub const RHYTHM_VARIATION_RANGE: std::ops::RangeInclusive<f32> = 0.3..=1.2;

impl Default for SpeechOptions {
    fn default() -> Self {
        Self {
            rate: 0.95,
            pitch: 0.0,
            expressiveness: DEFAULT_EXPRESSIVENESS,
            rhythm_variation: DEFAULT_RHYTHM_VARIATION,
        }
    }
}

impl SpeechOptions {
    fn factors(self) -> Result<(f32, f32), AudioError> {
        if !self.rate.is_finite()
            || !self.pitch.is_finite()
            || !(0.7..=1.5).contains(&self.rate)
            || !(-6.0..=6.0).contains(&self.pitch)
        {
            return Err(AudioError::Tts("invalid speech rate or pitch".into()));
        }
        let pitch = 2.0f32.powf(self.pitch / 12.0);
        Ok((self.rate / pitch, pitch))
    }
}

#[derive(Debug, Clone)]
enum Backend {
    Piper(PiperTts),
    Kokoro {
        exe: PathBuf,
        model: PathBuf,
        voice: KokoroVoice,
        length_scale: f32,
    },
    Cloud {
        config: CloudSpeechConfig,
        rate: f32,
    },
}

/// Prepared speech backend; synthesis always runs on an engine worker.
#[derive(Debug, Clone)]
pub struct Synthesizer {
    backend: Backend,
    pitch: f32,
}

impl Synthesizer {
    /// Prepares an installed local voice. No model is loaded until synthesis starts.
    pub fn local(
        store: &ModelStore,
        voice: Voice,
        options: SpeechOptions,
    ) -> Result<Self, AudioError> {
        let (rate, pitch) = options.factors()?;
        if !store.is_installed(voice.model_id()) || !store.is_installed(voice.runtime_id()) {
            return Err(AudioError::ModelMissing(voice.id().into()));
        }
        let backend = match voice {
            Voice::Piper(voice) => {
                let exe = store
                    .piper_exe()
                    .ok_or_else(|| AudioError::ModelMissing("piper".into()))?;
                let (model, json) = store.voice_files(voice);
                Backend::Piper(
                    PiperTts::new(exe, model, &json, 1.0 / rate)?
                        .with_variation(options.expressiveness, options.rhythm_variation),
                )
            }
            Voice::Kokoro(voice) => Backend::Kokoro {
                exe: store
                    .kokoro_exe()
                    .ok_or_else(|| AudioError::ModelMissing("sherpa-onnx".into()))?,
                model: store.kokoro_dir(),
                voice,
                length_scale: 1.0 / rate,
            },
        };
        Ok(Self { backend, pitch })
    }

    /// Prepares a speech service. Calling `synthesize` sends text to that service.
    pub fn cloud(config: CloudSpeechConfig, options: SpeechOptions) -> Result<Self, AudioError> {
        let (rate, pitch) = options.factors()?;
        if !(config.base_url.starts_with("http://") || config.base_url.starts_with("https://"))
            || config.model.trim().is_empty()
            || config.voice.trim().is_empty()
        {
            return Err(AudioError::Tts(
                "invalid speech endpoint configuration".into(),
            ));
        }
        Ok(Self {
            backend: Backend::Cloud { config, rate },
            pitch,
        })
    }

    /// Loads the voice ahead of the first sentence where that helps (Piper).
    pub fn warm_up(&self) {
        if let Backend::Piper(tts) = &self.backend {
            tts.warm_up();
        }
    }

    /// Generates a clip, checking cancellation while local child processes run.
    pub fn synthesize(
        &self,
        text: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<AudioClip, AudioError> {
        if cancelled() {
            return Err(AudioError::Cancelled);
        }
        let mut clip = match &self.backend {
            Backend::Piper(tts) => tts.synthesize_cancellable(text, cancelled)?,
            Backend::Kokoro {
                exe,
                model,
                voice,
                length_scale,
            } => kokoro(exe, model, *voice, *length_scale, text, cancelled)?,
            Backend::Cloud { config, rate } => cloud(config, *rate, text, cancelled)?,
        };
        if cancelled() {
            return Err(AudioError::Cancelled);
        }
        if (self.pitch - 1.0).abs() > f32::EPSILON {
            clip.samples = crate::resample(
                &clip.samples,
                (clip.sample_rate as f32 * self.pitch).round() as u32,
                clip.sample_rate,
            );
        }
        Ok(clip)
    }
}

fn path_arg(name: &str, path: &Path) -> OsString {
    let mut arg = OsString::from(format!("--{name}="));
    arg.push(path);
    arg
}

fn kokoro(
    exe: &Path,
    model: &Path,
    voice: KokoroVoice,
    length_scale: f32,
    text: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<AudioClip, AudioError> {
    let temp = tempfile::tempdir()
        .map_err(|e| AudioError::Tts(format!("could not create speech output: {e}")))?;
    let output = temp.path().join("speech.wav");
    // sherpa-onnx appends "/phontab" and similar to the data dir, and Windows
    // rejects forward slashes in `\\?\` paths, so pass a plain absolute path.
    let model = plain_path(std::fs::canonicalize(model).map_err(|e| AudioError::io(model, e))?);
    let exe = std::fs::canonicalize(exe).map_err(|e| AudioError::io(exe, e))?;
    let exe = crate::prepare_kokoro_executable(&exe)?;
    let mut command = Command::new(&exe);
    command
        .arg(path_arg("kokoro-model", &model.join("model.onnx")))
        .arg(path_arg("kokoro-voices", &model.join("voices.bin")))
        .arg(path_arg("kokoro-tokens", &model.join("tokens.txt")))
        .arg(path_arg("kokoro-data-dir", &model.join("espeak-ng-data")))
        .arg("--kokoro-lang=pt-br")
        .arg("--num-threads=2")
        .arg("--provider=cpu")
        .arg("--debug=0")
        .arg(format!("--sid={}", voice.speaker_id()))
        .arg(format!("--kokoro-length-scale={length_scale:.6}"))
        .arg(path_arg("output-filename", &output))
        .arg("--")
        .arg(text);
    run_child(&mut command, None, cancelled)?;
    let bytes = std::fs::read(&output).map_err(|e| AudioError::io(&output, e))?;
    decode_wav(&bytes)
}

/// Removes the `\\?\` prefix `canonicalize` adds on Windows when the rest is
/// an ordinary drive or network path.
pub(crate) fn plain_path(path: PathBuf) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path;
    };
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{unc}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path,
    }
}

fn cloud(
    config: &CloudSpeechConfig,
    rate: f32,
    text: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<AudioClip, AudioError> {
    let url = format!("{}/audio/speech", config.base_url.trim_end_matches('/'));
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(90)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut request = agent.post(&url);
    if !config.api_key.is_empty() {
        request = request.header("Authorization", format!("Bearer {}", config.api_key));
    }
    let body = serde_json::json!({"model": config.model, "voice": config.voice, "input": text, "speed": rate, "response_format": "wav"});
    let response = request
        .send_json(&body)
        .map_err(|_| AudioError::Tts("speech service request failed".into()))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(AudioError::Tts(format!(
            "speech service returned HTTP {status}"
        )));
    }
    let mut reader = response.into_body().into_reader();
    let mut bytes = Vec::new();
    let mut buffer = [0; 16 * 1024];
    loop {
        if cancelled() {
            return Err(AudioError::Cancelled);
        }
        let n = reader
            .read(&mut buffer)
            .map_err(|_| AudioError::Tts("could not read speech service audio".into()))?;
        if n == 0 {
            break;
        }
        if bytes.len() + n > 32 * 1024 * 1024 {
            return Err(AudioError::Tts(
                "speech service audio exceeds 32 MiB".into(),
            ));
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
    decode_wav(&bytes)
}

/// Decode and validate mono/stereo PCM or float speech without trusting its header.
pub(crate) fn decode_wav(bytes: &[u8]) -> Result<AudioClip, AudioError> {
    let bad = |e| AudioError::Tts(format!("invalid speech WAV: {e}"));
    let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes)).map_err(bad)?;
    let spec = reader.spec();
    if !(1..=2).contains(&spec.channels)
        || !(8_000..=192_000).contains(&spec.sample_rate)
        || !(1..=32).contains(&spec.bits_per_sample)
    {
        return Err(AudioError::Tts("unsupported speech WAV format".into()));
    }
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(bad)?,
        hound::SampleFormat::Int => {
            let divisor = 2.0f32.powi(i32::from(spec.bits_per_sample) - 1);
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|s| s as f32 / divisor))
                .collect::<Result<_, _>>()
                .map_err(bad)?
        }
    };
    if samples.is_empty()
        || !samples.len().is_multiple_of(usize::from(spec.channels))
        || samples.iter().any(|s| !s.is_finite())
    {
        return Err(AudioError::Tts(
            "speech WAV contains empty or invalid samples".into(),
        ));
    }
    let samples: Vec<f32> = samples
        .chunks_exact(usize::from(spec.channels))
        .map(|frame| frame.iter().sum::<f32>() / f32::from(spec.channels))
        .collect();
    Ok(AudioClip {
        samples: crate::soften(&samples, spec.sample_rate),
        sample_rate: spec.sample_rate,
    })
}

struct RunningChild(Child);
impl Drop for RunningChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// File-backed output avoids pipe deadlocks, and Drop reaps failed/cancelled children.
pub(crate) fn run_child(
    command: &mut Command,
    input: Option<&str>,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>, AudioError> {
    if cancelled() {
        return Err(AudioError::Cancelled);
    }
    let fail = |e| AudioError::Tts(format!("speech process failed: {e}"));
    let mut output = tempfile::tempfile().map_err(fail)?;
    let mut error = tempfile::tempfile().map_err(fail)?;
    command
        .stdout(output.try_clone().map_err(fail)?)
        .stderr(error.try_clone().map_err(fail)?)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = RunningChild(command.spawn().map_err(fail)?);
    if let Some(input) = input
        && let Some(mut stdin) = child.0.stdin.take()
    {
        stdin.write_all(input.as_bytes()).map_err(fail)?;
    }
    let started = Instant::now();
    loop {
        if cancelled() {
            return Err(AudioError::Cancelled);
        }
        if started.elapsed() > Duration::from_secs(120) {
            return Err(AudioError::Tts("speech process timed out".into()));
        }
        if let Some(status) = child.0.try_wait().map_err(fail)? {
            if !status.success() {
                error.seek(SeekFrom::Start(0)).map_err(fail)?;
                let mut detail = String::new();
                error
                    .take(8_192)
                    .read_to_string(&mut detail)
                    .map_err(fail)?;
                return Err(AudioError::Tts(format!(
                    "speech process exited with {status}: {}",
                    detail.trim()
                )));
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    output.seek(SeekFrom::Start(0)).map_err(fail)?;
    let mut bytes = Vec::new();
    output
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(fail)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(AudioError::Tts("speech output exceeds 32 MiB".into()));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave_bytes() -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        let mut writer = hound::WavWriter::new(
            &mut out,
            hound::WavSpec {
                channels: 1,
                sample_rate: 24_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for sample in [0i16, 1000, -1000, 0] {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        out.into_inner()
    }

    #[test]
    fn plain_path_drops_the_verbatim_prefix() {
        assert_eq!(
            plain_path(PathBuf::from(r"\\?\C:\Users\a\kokoro")),
            PathBuf::from(r"C:\Users\a\kokoro")
        );
        assert_eq!(
            plain_path(PathBuf::from(r"\\?\UNC\server\share\m")),
            PathBuf::from(r"\\server\share\m")
        );
        assert_eq!(
            plain_path(PathBuf::from("/opt/kokoro")),
            PathBuf::from("/opt/kokoro")
        );
    }

    #[test]
    fn speech_service_receives_expected_contract_and_decodes_audio() {
        use std::io::BufRead;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap();
                }
                request.push_str(&line);
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let wave = wave_bytes();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", wave.len()).unwrap();
            stream.write_all(&wave).unwrap();
            (
                request,
                serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            )
        });
        let config = CloudSpeechConfig {
            base_url: format!("http://{address}/v1/"),
            api_key: "test-token".into(),
            ..CloudSpeechConfig::default()
        };
        let tts = Synthesizer::cloud(config, SpeechOptions::default()).unwrap();
        let clip = tts
            .synthesize("A crase une duas vogais.", &|| false)
            .unwrap();
        let (headers, body) = server.join().unwrap();
        assert!(headers.starts_with("POST /v1/audio/speech HTTP/1.1"));
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("authorization: bearer test-token")
        );
        assert_eq!(body["input"], "A crase une duas vogais.");
        assert_eq!(body["response_format"], "wav");
        assert_eq!(body["voice"], "pf_dora");
        assert_eq!(clip.sample_rate, 24_000);
        assert!(!clip.samples.is_empty());
    }

    #[test]
    fn cancelled_speech_does_not_spawn_a_process() {
        let mut command = Command::new("nonexistent-speech-executable");
        assert!(matches!(
            run_child(&mut command, None, &|| true),
            Err(AudioError::Cancelled)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn cancellation_reaps_running_process() {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 10",
        ]);
        let start = Instant::now();
        let result = run_child(&mut command, None, &|| {
            start.elapsed() > Duration::from_millis(100)
        });
        assert!(matches!(result, Err(AudioError::Cancelled)));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn invalid_or_truncated_wav_is_rejected() {
        assert!(decode_wav(b"not a wave").is_err());
        assert!(decode_wav(b"RIFF\x24\0\0\0WAVE").is_err());
    }

    #[test]
    fn pitch_compensates_synthesis_rate() {
        let (rate, pitch) = SpeechOptions {
            rate: 1.2,
            pitch: 6.0,
            ..SpeechOptions::default()
        }
        .factors()
        .unwrap();
        assert!((rate * pitch - 1.2).abs() < 0.0001);
        assert!(
            SpeechOptions {
                rate: f32::NAN,
                ..SpeechOptions::default()
            }
            .factors()
            .is_err()
        );
    }
}
