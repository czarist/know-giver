//! Speech recognition with NVIDIA Nemotron 3.5 ASR Streaming 0.6B, run by the
//! official NeMo-Speech.cpp server (`nemo-speech serve`) as a child process.
//! The language is chosen explicitly (pt-BR), so the model never guesses it.
//! Nothing leaves the computer: the server only listens on 127.0.0.1.
//!
//! The runtime build and the model are pinned with checksums in
//! `local-ai.toml`, embedded here. Where NVIDIA publishes a GPU build
//! (Vulkan on Windows and Linux, Metal on Apple silicon; Intel Macs have
//! none), it is used when `nemo-speech doctor` reports a real GPU with room
//! for the model; otherwise, or when it fails to start, the CPU build runs.
//! A GPU build that crashed while loading is remembered and skipped later.
//! `KG_NEMOTRON_BACKEND=cpu` or `=gpu` forces one, for measurements.

use std::io::{Cursor, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::signal::{prepare_for_stt, resample};
use crate::{AudioClip, AudioError, STT_SAMPLE_RATE};

const MANIFEST_TEXT: &str = include_str!("../../../local-ai.toml");
/// Loading the model takes a few seconds; give slow disks some room.
const START_TIMEOUT: Duration = Duration::from_secs(90);
const REPLY_TIMEOUT: Duration = Duration::from_secs(60);
const BOUNDARY: &str = "know-giver-nemotron-boundary";
/// Our own copy of the server executable on Windows, so a server left
/// behind by a crash can be recognized and stopped by name.
#[cfg(windows)]
const SERVER_COPY: &str = "know-giver-nemotron.exe";

#[derive(Debug, Deserialize)]
struct Manifest {
    nemo_speech: RuntimeManifest,
    stt: SttManifest,
}

#[derive(Debug, Deserialize)]
struct RuntimeManifest {
    repository: String,
    version: String,
    builds: std::collections::BTreeMap<String, Build>,
}

#[derive(Debug, Deserialize)]
struct Build {
    file: String,
    sha256: String,
    gpu_file: Option<String>,
    gpu_sha256: Option<String>,
    cuda_file: Option<String>,
    cuda_sha256: Option<String>,
    cuda_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct SttManifest {
    nemotron_3_5: ModelManifest,
}

#[derive(Debug, Deserialize)]
struct ModelManifest {
    repository: String,
    revision: String,
    model_file: String,
    model_bytes: u64,
    model_sha256: String,
    language: String,
}

fn manifest() -> Result<&'static Manifest, AudioError> {
    static MANIFEST: OnceLock<Result<Manifest, String>> = OnceLock::new();
    MANIFEST
        .get_or_init(|| toml::from_str(MANIFEST_TEXT).map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| AudioError::Model(format!("local-ai.toml: {e}")))
}

/// Key of this system in `[nemo_speech.builds]`.
fn platform_key() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("windows-x86_64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x86_64")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("linux-aarch64")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("macos-aarch64")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("macos-x86_64")
    } else {
        None
    }
}

/// The pinned NeMo-Speech.cpp build for this system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimePackage {
    pub(crate) url: String,
    pub(crate) sha256: String,
    /// Top folder inside the archive.
    pub(crate) dir: String,
}

/// Which build of the runtime runs the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NemotronBackend {
    /// CUDA build for NVIDIA GPUs (Windows, Linux x86_64).
    Cuda,
    /// Vulkan build (Windows, Linux) or Metal build (macOS).
    Gpu,
    /// CPU build.
    Cpu,
}

impl NemotronBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::Gpu => "gpu",
            Self::Cpu => "cpu",
        }
    }

    /// Folder under the runtime directory holding this build.
    pub(crate) fn folder(self) -> &'static str {
        self.as_str()
    }

    /// Value of `--device` for `nemo-speech serve`.
    fn device(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::Gpu if cfg!(target_os = "macos") => "metal",
            Self::Gpu => "vulkan",
            Self::Cpu => "cpu",
        }
    }
}

/// An installed runtime build to try.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NemotronRuntime {
    pub exe: PathBuf,
    pub backend: NemotronBackend,
}

/// The pinned build of `backend` for this system, if NVIDIA publishes one.
pub(crate) fn runtime_package(backend: NemotronBackend) -> Option<RuntimePackage> {
    let manifest = manifest().ok()?;
    let build = manifest.nemo_speech.builds.get(platform_key()?)?;
    let (file, sha256) = match backend {
        NemotronBackend::Cpu => (&build.file, &build.sha256),
        NemotronBackend::Gpu => (build.gpu_file.as_ref()?, build.gpu_sha256.as_ref()?),
        NemotronBackend::Cuda => (build.cuda_file.as_ref()?, build.cuda_sha256.as_ref()?),
    };
    let dir = file
        .strip_suffix(".zip")
        .or_else(|| file.strip_suffix(".tar.gz"))?
        .to_owned();
    Some(RuntimePackage {
        url: format!(
            "{}/releases/download/{}/{}",
            manifest.nemo_speech.repository, manifest.nemo_speech.version, file
        ),
        sha256: sha256.clone(),
        dir,
    })
}

/// The order to try the builds in: CUDA, then Vulkan or Metal, then CPU,
/// unless `KG_NEMOTRON_BACKEND` asks for only one.
pub(crate) fn backend_order() -> Vec<NemotronBackend> {
    match forced_backend() {
        Some(backend) => vec![backend],
        None => vec![
            NemotronBackend::Cuda,
            NemotronBackend::Gpu,
            NemotronBackend::Cpu,
        ],
    }
}

impl NemotronBackend {
    /// Reads back [`Self::as_str`].
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "cuda" => Some(Self::Cuda),
            "gpu" => Some(Self::Gpu),
            "cpu" => Some(Self::Cpu),
            _ => None,
        }
    }
}

/// Whether an NVIDIA driver is installed, without starting anything:
/// `nvcuda.dll` on Windows, `/proc/driver/nvidia` on Linux. It decides
/// whether the CUDA build is downloaded (and counted in the download size).
pub fn nvidia_driver_present() -> bool {
    static PRESENT: OnceLock<bool> = OnceLock::new();
    *PRESENT.get_or_init(|| {
        if cfg!(windows) {
            let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
            Path::new(&root)
                .join("System32")
                .join("nvcuda.dll")
                .is_file()
        } else if cfg!(target_os = "linux") {
            Path::new("/proc/driver/nvidia/version").is_file()
        } else {
            false
        }
    })
}

/// Size of the CUDA build to download here: zero without an NVIDIA driver
/// or an official CUDA build for this system.
pub(crate) fn cuda_download_bytes() -> u64 {
    if !nvidia_driver_present() {
        return 0;
    }
    manifest()
        .ok()
        .zip(platform_key())
        .and_then(|(manifest, key)| manifest.nemo_speech.builds.get(key))
        .and_then(|build| build.cuda_file.as_ref().and(build.cuda_bytes))
        .unwrap_or(0)
}

/// Held while timing builds or starting a server: on Windows each start
/// stops leftover server copies by name, so two at once would stop each
/// other (and a crash marker could be left for a build that is fine).
pub(crate) fn start_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn forced_backend() -> Option<NemotronBackend> {
    std::env::var("KG_NEMOTRON_BACKEND")
        .ok()
        .and_then(|value| NemotronBackend::parse(&value))
}

/// Smallest free GPU memory worth trying: the q8_0 model plus its buffers.
const MIN_GPU_FREE_BYTES: u64 = 1536 * 1024 * 1024;
const DOCTOR_TIMEOUT: Duration = Duration::from_secs(20);
/// Left next to a GPU build that crashed while loading. Reinstalling the
/// build (or a new pinned version) removes it with the old folder.
const GPU_FAILED_MARKER: &str = "know-giver-gpu-failed.txt";

#[derive(Debug, Deserialize)]
struct Doctor {
    #[serde(default)]
    devices: Vec<DoctorDevice>,
    /// False when the GPU driver is too old for the bundled runtime (CUDA).
    driver_runtime_compatible: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DoctorDevice {
    #[serde(default)]
    description: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    memory_free: u64,
}

/// The first usable GPU in a `nemo-speech doctor --json` report.
#[cfg(test)]
fn usable_gpu(report: &str) -> Option<String> {
    usable_gpus(report).into_iter().next()
}

/// Usable GPUs in a `nemo-speech doctor --json` report: not a software
/// renderer, with room for the model when memory is reported, and a driver
/// the runtime accepts.
fn usable_gpus(report: &str) -> Vec<String> {
    let Ok(doctor) = serde_json::from_str::<Doctor>(report) else {
        return Vec::new();
    };
    if doctor.driver_runtime_compatible == Some(false) {
        return Vec::new();
    }
    doctor
        .devices
        .into_iter()
        .filter(|device| {
            let name = device.description.to_lowercase();
            let software = [
                "llvmpipe",
                "lavapipe",
                "swiftshader",
                "basic render",
                "softpipe",
            ]
            .iter()
            .any(|s| name.contains(s));
            device.kind == "gpu"
                && !software
                && (device.memory_free == 0 || device.memory_free >= MIN_GPU_FREE_BYTES)
        })
        .map(|device| device.description.trim().to_owned())
        .collect()
}

/// The first usable GPU the build at `exe` sees.
fn probe_gpu(exe: &Path) -> Option<String> {
    probe_gpus(exe).into_iter().next()
}

/// Asks the GPU build which devices it sees. Empty when it finds no usable
/// GPU, crashes or takes too long.
fn probe_gpus(exe: &Path) -> Vec<String> {
    probe_report(exe).map_or_else(Vec::new, |report| usable_gpus(&report))
}

fn probe_report(exe: &Path) -> Option<String> {
    let mut command = Command::new(exe);
    command
        .args(["doctor", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < DOCTOR_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(100));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    reader.join().ok()
}

fn gpu_failed_marker(exe: &Path) -> Option<PathBuf> {
    exe.parent().map(|dir| dir.join(GPU_FAILED_MARKER))
}

/// Whether to try the GPU build at `exe`: forced, or the build sees a real
/// GPU and has not crashed here before.
fn should_try_gpu(exe: &Path, backend: NemotronBackend) -> bool {
    let name = backend.as_str();
    if forced_backend() == Some(backend) {
        return true;
    }
    if gpu_failed_marker(exe).is_some_and(|marker| marker.is_file()) {
        tracing::info!(
            backend = name,
            "nemotron build crashed here before; skipped"
        );
        return false;
    }
    match probe_gpu(exe) {
        Some(gpu) => {
            tracing::info!(backend = name, %gpu, "nemotron GPU found");
            true
        }
        None => {
            tracing::info!(backend = name, "no usable GPU for this nemotron build");
            false
        }
    }
}

/// The pinned Nemotron model file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelFile {
    pub(crate) url: String,
    pub(crate) sha256: String,
    pub(crate) file: String,
    pub(crate) bytes: u64,
}

pub(crate) fn model_file() -> Result<ModelFile, AudioError> {
    let model = &manifest()?.stt.nemotron_3_5;
    Ok(ModelFile {
        url: format!(
            "{}/resolve/{}/{}",
            model.repository, model.revision, model.model_file
        ),
        sha256: model.model_sha256.clone(),
        file: model.model_file.clone(),
        bytes: model.model_bytes,
    })
}

/// Language sent with every request.
fn language() -> Result<&'static str, AudioError> {
    Ok(manifest()?.stt.nemotron_3_5.language.as_str())
}

/// A running NeMo-Speech server with the Nemotron model loaded.
pub struct NemotronStt {
    child: Child,
    agent: ureq::Agent,
    base: String,
    language: &'static str,
    backend: NemotronBackend,
}

impl std::fmt::Debug for NemotronStt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NemotronStt")
            .field("pid", &self.child.id())
            .field("base", &self.base)
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

impl NemotronStt {
    /// Starts the first runtime that gets ready with the GGUF `model`:
    /// usually the GPU build, then the CPU build when that fails.
    pub fn start(runtimes: &[NemotronRuntime], model: &Path) -> Result<Self, AudioError> {
        if !model.is_file() {
            return Err(AudioError::ModelMissing(model.display().to_string()));
        }
        let _start = start_lock();
        let mut failures = Vec::new();
        for runtime in runtimes {
            if runtime.backend != NemotronBackend::Cpu
                && !should_try_gpu(&runtime.exe, runtime.backend)
            {
                continue;
            }
            match Self::start_one(&runtime.exe, runtime.backend, model) {
                Ok(stt) => return Ok(stt),
                Err(StartError { err, crashed }) => {
                    tracing::warn!(backend = runtime.backend.as_str(), %err, "nemotron runtime failed to start");
                    if crashed {
                        remember_crash(runtime, &err);
                    }
                    failures.push(format!("{}: {err}", runtime.backend.as_str()));
                }
            }
        }
        if failures.is_empty() {
            return Err(AudioError::ModelMissing("nemo-speech".into()));
        }
        Err(AudioError::Model(failures.join("; ")))
    }

    /// Which build is running.
    pub fn backend(&self) -> NemotronBackend {
        self.backend
    }

    /// Process id of the server, to measure its memory.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    fn start_one(exe: &Path, backend: NemotronBackend, model: &Path) -> Result<Self, StartError> {
        let started = Instant::now();
        let language = language()?;
        let exe = server_executable(exe)?;
        let port = free_port()?;
        let log =
            std::env::temp_dir().join(format!("know-giver-nemotron-{}.log", backend.as_str()));
        let log_file = std::fs::File::create(&log).map_err(|e| AudioError::io(&log, e))?;
        let log_err = log_file.try_clone().map_err(|e| AudioError::io(&log, e))?;

        let mut command = Command::new(&exe);
        command
            .arg("serve")
            .arg("--asr-model")
            .arg(crate::tts::plain_path(model.to_path_buf()))
            .args(["--host", "127.0.0.1"])
            .arg("--port")
            .arg(port.to_string())
            .args(["--device", backend.device()])
            .args(["--threads", "2"])
            .arg("--no-ui")
            // The built-in warmup aborts the v0.3.0 Vulkan build
            // (GGML_ASSERT in ggml-cpu); the first request below does it.
            .arg("--no-warmup")
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_err));
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
            .map_err(|e| AudioError::Model(format!("could not start nemo-speech: {e}")))?;

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(2)))
            .timeout_global(Some(REPLY_TIMEOUT))
            .build()
            .into();
        let base = format!("http://127.0.0.1:{port}");
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(StartError {
                    err: AudioError::Model(format!(
                        "nemo-speech stopped while loading ({status}); see {}",
                        log.display()
                    )),
                    crashed: true,
                });
            }
            if started.elapsed() > START_TIMEOUT {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AudioError::Model(format!(
                    "nemo-speech did not get ready; see {}",
                    log.display()
                ))
                .into());
            }
            if agent
                .get(format!("{base}/ready"))
                .call()
                .is_ok_and(|r| r.status().is_success())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let mut stt = Self {
            child,
            agent,
            base,
            language,
            backend,
        };
        // The first request pays for kernel setup; spend it on a second of
        // faint noise instead of on the student's first answer. A server
        // that dies here is no better than one that never started.
        if let Err(err) = stt.post(&warm_up_audio()) {
            let crashed = !stt.is_running();
            return Err(StartError {
                err: AudioError::Model(format!(
                    "nemo-speech failed its first request ({err}); see {}",
                    log.display()
                )),
                crashed,
            });
        }
        tracing::info!(
            port,
            language,
            backend = backend.as_str(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "nemotron ready"
        );
        Ok(stt)
    }

    /// True while the server process is alive.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Transcribes a clip in the configured language. Returns an empty
    /// string for silence.
    pub fn transcribe(&mut self, clip: &AudioClip) -> Result<String, AudioError> {
        let pcm = prepare_for_stt(
            &resample(&clip.samples, clip.sample_rate, STT_SAMPLE_RATE),
            STT_SAMPLE_RATE,
        );
        if pcm.len() < STT_SAMPLE_RATE as usize / 2 {
            return Ok(String::new());
        }
        let started = Instant::now();
        let text = self.post(&pcm)?;
        tracing::info!(
            audio_secs = clip.duration_secs(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            chars = text.len(),
            "transcribed with nemotron"
        );
        Ok(text)
    }

    /// Sends 16 kHz mono samples as they are and returns the text.
    fn post(&mut self, pcm: &[f32]) -> Result<String, AudioError> {
        let body = multipart(&wav_bytes(pcm)?, self.language);
        let response = self
            .agent
            .post(format!("{}/v1/audio/transcriptions", self.base))
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .send(&body[..])
            .map_err(|e| AudioError::Model(format!("nemo-speech: {e}")))?;
        let mut reply = String::new();
        response
            .into_body()
            .into_reader()
            .take(1024 * 1024)
            .read_to_string(&mut reply)
            .map_err(|e| AudioError::Model(format!("nemo-speech reply: {e}")))?;
        parse_reply(&reply)
    }
}

/// Why a runtime did not start; `crashed` when the process itself died.
struct StartError {
    err: AudioError,
    crashed: bool,
}

impl From<AudioError> for StartError {
    fn from(err: AudioError) -> Self {
        Self {
            err,
            crashed: false,
        }
    }
}

/// One second of faint noise at 16 kHz, the same every time.
fn warm_up_audio() -> Vec<f32> {
    noise(STT_SAMPLE_RATE as usize, 0.002)
}

/// Deterministic white noise at 16 kHz with peak `amplitude / 2`.
fn noise(samples: usize, amplitude: f32) -> Vec<f32> {
    let mut state: u32 = 0x2545_f491;
    (0..samples)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32 - 0.5) * amplitude
        })
        .collect()
}

/// Audio timed per build when choosing the fastest: as long as a typical
/// answer. The encoder's work depends on the length, not on the words.
const BENCH_SAMPLES: usize = 4 * STT_SAMPLE_RATE as usize;

/// Starts each usable build in turn, times it on the same four seconds of
/// audio (best of two) and returns the fastest. Builds that crash are
/// remembered like in [`NemotronStt::start`]. The caller holds
/// [`start_lock`].
pub(crate) fn fastest_backend(
    runtimes: &[NemotronRuntime],
    model: &Path,
) -> Option<NemotronBackend> {
    if !model.is_file() {
        return None;
    }
    let audio = noise(BENCH_SAMPLES, 0.1);
    let mut timings: Vec<(NemotronBackend, Duration)> = Vec::new();
    for runtime in runtimes {
        if runtime.backend != NemotronBackend::Cpu && !should_try_gpu(&runtime.exe, runtime.backend)
        {
            continue;
        }
        let mut stt = match NemotronStt::start_one(&runtime.exe, runtime.backend, model) {
            Ok(stt) => stt,
            Err(StartError { err, crashed }) => {
                tracing::warn!(backend = runtime.backend.as_str(), %err, "nemotron build failed while timing");
                if crashed {
                    remember_crash(runtime, &err);
                }
                continue;
            }
        };
        let mut time: Option<Duration> = None;
        for _ in 0..2 {
            let started = Instant::now();
            if stt.post(&audio).is_err() {
                time = None;
                break;
            }
            let elapsed = started.elapsed();
            time = Some(time.map_or(elapsed, |t| t.min(elapsed)));
        }
        drop(stt);
        let Some(time) = time else {
            continue;
        };
        tracing::info!(
            backend = runtime.backend.as_str(),
            ms = time.as_millis() as u64,
            "nemotron build timed"
        );
        timings.push((runtime.backend, time));
    }
    pick_fastest(&timings)
}

/// The fastest timed build. A GPU build must beat the CPU build by 20% to
/// be picked over it: a near tie is noise, and the CPU build keeps
/// punctuation that the v0.3.0 Vulkan build drops.
fn pick_fastest(timings: &[(NemotronBackend, Duration)]) -> Option<NemotronBackend> {
    let (best, best_time) = *timings.iter().min_by_key(|(_, time)| *time)?;
    let cpu = timings
        .iter()
        .find(|(backend, _)| *backend == NemotronBackend::Cpu);
    match cpu {
        Some((_, cpu_time)) if best_time.mul_f32(1.25) > *cpu_time => Some(NemotronBackend::Cpu),
        _ => Some(best),
    }
}

/// Marks a GPU build that crashed, so it is skipped from then on.
fn remember_crash(runtime: &NemotronRuntime, err: &AudioError) {
    if runtime.backend != NemotronBackend::Cpu
        && let Some(marker) = gpu_failed_marker(&runtime.exe)
    {
        let _ = std::fs::write(&marker, err.to_string());
    }
}

impl Drop for NemotronStt {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 16 kHz mono PCM16 WAV in memory.
fn wav_bytes(pcm: &[f32]) -> Result<Vec<u8>, AudioError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: STT_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let fail = |e: hound::Error| AudioError::Model(format!("could not encode audio: {e}"));
    let mut cursor = Cursor::new(Vec::with_capacity(44 + pcm.len() * 2));
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec).map_err(fail)?;
        for sample in pcm {
            let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
            writer.write_sample(value).map_err(fail)?;
        }
        writer.finalize().map_err(fail)?;
    }
    Ok(cursor.into_inner())
}

/// The form `POST /v1/audio/transcriptions` takes: the WAV file, the
/// language and a plain JSON reply.
fn multipart(wav: &[u8], language: &str) -> Vec<u8> {
    let mut body = Vec::with_capacity(wav.len() + 512);
    let field = |body: &mut Vec<u8>, name: &str, value: &str| {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    };
    field(&mut body, "language", language);
    field(&mut body, "response_format", "json");
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"turn.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(wav);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    body
}

fn parse_reply(reply: &str) -> Result<String, AudioError> {
    #[derive(Deserialize)]
    struct Reply {
        text: String,
    }
    let parsed: Reply = serde_json::from_str(reply)
        .map_err(|e| AudioError::Model(format!("unexpected nemo-speech reply: {e}")))?;
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
    fn doctor_report_picks_a_real_gpu_with_room() {
        // Trimmed from `nemo-speech doctor --json` (v0.3.0, Windows Vulkan).
        let report = r#"{"accelerator_available": true, "devices": [
            {"description": "NVIDIA GeForce RTX 3060", "index": 0,
             "memory_free": 12110446592, "memory_total": 12927893504,
             "name": "Vulkan0", "type": "gpu"},
            {"description": "AMD Ryzen 7 5700X 8-Core Processor", "index": 1,
             "memory_free": 14104530944, "name": "CPU", "type": "cpu"}],
            "version": "0.3.0"}"#;
        assert_eq!(
            usable_gpu(report).as_deref(),
            Some("NVIDIA GeForce RTX 3060")
        );
        let cpu_only = r#"{"devices": [{"description": "AMD Ryzen", "type": "cpu"}]}"#;
        assert_eq!(usable_gpu(cpu_only), None);
        let software = r#"{"devices": [{"description": "llvmpipe (LLVM 15.0.7, 256 bits)",
            "memory_free": 8000000000, "type": "gpu"}]}"#;
        assert_eq!(usable_gpu(software), None);
        let full = r#"{"devices": [{"description": "Intel(R) UHD Graphics",
            "memory_free": 500000000, "type": "gpu"}]}"#;
        assert_eq!(usable_gpu(full), None);
        let unknown_memory = r#"{"devices": [{"description": "Apple M1", "type": "gpu"}]}"#;
        assert_eq!(usable_gpu(unknown_memory).as_deref(), Some("Apple M1"));
        assert_eq!(usable_gpu("not json"), None);
        let old_driver = r#"{"driver_runtime_compatible": false, "devices": [
            {"description": "NVIDIA GeForce GTX 1060", "type": "gpu"}]}"#;
        assert_eq!(usable_gpu(old_driver), None);
        let two = r#"{"devices": [
            {"description": "AMD Radeon(TM) Graphics", "type": "gpu"},
            {"description": "NVIDIA GeForce RTX 4060 Laptop GPU", "type": "gpu"}]}"#;
        assert_eq!(usable_gpus(two).len(), 2);
    }

    #[test]
    fn fastest_build_needs_a_clear_lead_over_the_cpu() {
        use NemotronBackend::{Cpu, Cuda, Gpu};
        let ms = Duration::from_millis;
        // Measured on an RTX 3060 / Ryzen 7 5700X.
        assert_eq!(
            pick_fastest(&[(Cuda, ms(18)), (Gpu, ms(254)), (Cpu, ms(228))]),
            Some(Cuda)
        );
        assert_eq!(pick_fastest(&[(Gpu, ms(270)), (Cpu, ms(257))]), Some(Cpu));
        assert_eq!(pick_fastest(&[(Gpu, ms(220)), (Cpu, ms(257))]), Some(Cpu));
        assert_eq!(pick_fastest(&[(Gpu, ms(150)), (Cpu, ms(600))]), Some(Gpu));
        assert_eq!(pick_fastest(&[(Gpu, ms(300))]), Some(Gpu));
        assert_eq!(pick_fastest(&[]), None);
        assert_eq!(NemotronBackend::parse("cuda\n"), Some(Cuda));
        assert_eq!(NemotronBackend::parse("vulkan"), None);
    }

    /// A program that exits at once with an error, standing in for a build
    /// that crashes while loading.
    fn failing_program(dir: &Path) -> PathBuf {
        let exe = dir.join(if cfg!(windows) {
            "nemo-speech.exe"
        } else {
            "nemo-speech"
        });
        let source = if cfg!(windows) {
            PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/whoami.exe")
        } else {
            // macOS has it only in /usr/bin; most Linux systems have both.
            ["/usr/bin/false", "/bin/false"]
                .into_iter()
                .map(PathBuf::from)
                .find(|path| path.is_file())
                .unwrap()
        };
        std::fs::copy(source, &exe).unwrap();
        exe
    }

    #[test]
    fn crashed_builds_fail_fast_and_gpu_ones_are_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("model.gguf");
        std::fs::write(&model, b"gguf").unwrap();
        let exe = failing_program(dir.path());
        let cpu = NemotronRuntime {
            exe: exe.clone(),
            backend: NemotronBackend::Cpu,
        };
        let started = Instant::now();
        let err = NemotronStt::start(std::slice::from_ref(&cpu), &model).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(20), "{err}");
        assert!(err.to_string().contains("cpu:"), "{err}");
        // A CPU build is never marked: it is the last resort.
        let marker = gpu_failed_marker(&exe).unwrap();
        assert!(!marker.exists());
        // `doctor` fails too, so the GPU build is skipped without starting.
        let gpu = NemotronRuntime {
            exe: exe.clone(),
            backend: NemotronBackend::Gpu,
        };
        assert!(probe_gpu(&exe).is_none());
        assert!(matches!(
            NemotronStt::start(std::slice::from_ref(&gpu), &model),
            Err(AudioError::ModelMissing(_))
        ));
        // A GPU build that crashed is skipped from then on.
        remember_crash(&gpu, &AudioError::Model("crashed".into()));
        assert!(marker.is_file());
        assert!(!should_try_gpu(&exe, NemotronBackend::Gpu) || forced_backend().is_some());
        remember_crash(&cpu, &AudioError::Model("crashed".into()));
        assert_eq!(
            std::fs::read_to_string(&marker).unwrap(),
            "speech model failure: crashed"
        );
        // A missing model is reported as such, before any start.
        assert!(matches!(
            NemotronStt::start(&[cpu], &dir.path().join("none.gguf")),
            Err(AudioError::ModelMissing(_))
        ));
        assert_eq!(fastest_backend(&[], &model), None);
    }

    #[test]
    fn warm_up_audio_is_one_faint_second() {
        let audio = warm_up_audio();
        assert_eq!(audio.len(), STT_SAMPLE_RATE as usize);
        assert!(audio.iter().all(|s| s.abs() <= 0.001));
        assert!(audio.iter().any(|s| *s != 0.0));
    }

    #[test]
    fn manifest_pins_runtime_and_model() {
        let model = model_file().unwrap();
        assert!(model.url.starts_with("https://huggingface.co/nvidia/"));
        assert!(model.url.ends_with(".gguf"));
        assert_eq!(model.sha256.len(), 64);
        assert!(model.bytes > 100_000_000);
        assert_eq!(language().unwrap(), "pt-BR");
        let manifest = manifest().unwrap();
        for build in manifest.nemo_speech.builds.values() {
            assert_eq!(build.sha256.len(), 64);
            assert!(build.file.ends_with(".zip") || build.file.ends_with(".tar.gz"));
        }
        for build in manifest.nemo_speech.builds.values() {
            assert_eq!(build.gpu_file.is_some(), build.gpu_sha256.is_some());
            assert_eq!(build.cuda_file.is_some(), build.cuda_sha256.is_some());
            assert_eq!(build.cuda_file.is_some(), build.cuda_bytes.is_some());
            for sha in [&build.gpu_sha256, &build.cuda_sha256]
                .into_iter()
                .flatten()
            {
                assert_eq!(sha.len(), 64);
            }
        }
        if platform_key().is_some() {
            let package = runtime_package(NemotronBackend::Cpu).unwrap();
            assert!(
                package
                    .url
                    .starts_with("https://github.com/NVIDIA/NeMo-Speech.cpp/releases/download/v")
            );
            assert!(package.dir.starts_with("nemo-speech-"));
            assert!(package.dir.ends_with("-cpu"));
        }
        if cfg!(all(windows, target_arch = "x86_64")) {
            let gpu = runtime_package(NemotronBackend::Gpu).unwrap();
            assert!(gpu.dir.ends_with("-vulkan"));
            let cuda = runtime_package(NemotronBackend::Cuda).unwrap();
            assert!(cuda.dir.ends_with("-cuda"));
        }
        if cfg!(target_os = "macos") {
            assert!(runtime_package(NemotronBackend::Cuda).is_none());
        }
    }

    #[test]
    fn wav_is_pcm16_mono_16k() {
        let wav = wav_bytes(&[0.0, 0.5, -1.0]).unwrap();
        let reader = hound::WavReader::new(Cursor::new(wav)).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, STT_SAMPLE_RATE);
        assert_eq!(spec.bits_per_sample, 16);
        let samples: Vec<i16> = reader.into_samples().map(Result::unwrap).collect();
        assert_eq!(samples, [0, 16383, -32767]);
    }

    #[test]
    fn form_has_language_format_and_file() {
        let body = String::from_utf8_lossy(&multipart(b"RIFF", "pt-BR")).into_owned();
        assert!(body.contains("name=\"language\"\r\n\r\npt-BR\r\n"));
        assert!(body.contains("name=\"response_format\"\r\n\r\njson\r\n"));
        assert!(body.contains("filename=\"turn.wav\""));
        assert!(body.ends_with(&format!("RIFF\r\n--{BOUNDARY}--\r\n")));
    }

    #[test]
    fn reply_text_is_trimmed_and_noise_dropped() {
        assert_eq!(
            parse_reply(r#"{"text": " Vou à escola. "}"#).unwrap(),
            "Vou à escola."
        );
        assert_eq!(parse_reply(r#"{"text": "."}"#).unwrap(), "");
        assert!(parse_reply(r#"{"error": {"message": "x"}}"#).is_err());
    }
}
