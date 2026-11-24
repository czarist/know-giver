//! Local manuscript OCR using the pinned PaddleOCR-VL model and llama.cpp.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{AppPaths, Engine, Event};

const MANIFEST_TEXT: &str = include_str!("../../../local-ai.toml");
const MODEL_ID: &str = "paddleocr-vl-1.5";
const COMPLETIONS_PATH: &str = "/v1/chat/completions";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const VERIFIED_FILE: &str = "verified.sha256";
const SERVER_NAME: &str = if cfg!(windows) {
    "llama-server.exe"
} else {
    "llama-server"
};
static MANIFEST: std::sync::OnceLock<Result<Manifest, String>> = std::sync::OnceLock::new();

#[derive(Debug, Deserialize)]
struct Manifest {
    llama_cpp: LlamaManifest,
    ocr: OcrManifest,
}

#[derive(Debug, Deserialize)]
struct LlamaManifest {
    repository: String,
    version: String,
    commit: String,
    license: String,
    /// Release tag of the same commit, where the official builds live.
    release: String,
    /// Official CUDA builds by platform (`windows-x86_64`, `linux-x86_64`).
    #[serde(default)]
    cuda: std::collections::HashMap<String, CudaBuild>,
}

/// An official llama.cpp CUDA build: llama-server and the CUDA libraries it
/// loads, two archives unpacked into one folder.
#[derive(Debug, Deserialize)]
struct CudaBuild {
    file: String,
    sha256: String,
    bytes: u64,
    runtime_file: String,
    runtime_sha256: String,
    runtime_bytes: u64,
}

impl CudaBuild {
    fn total_bytes(&self) -> u64 {
        self.bytes + self.runtime_bytes
    }

    fn marker(&self) -> String {
        format!("{}\n{}\n", self.sha256, self.runtime_sha256)
    }
}

#[derive(Debug, Deserialize)]
struct OcrManifest {
    paddleocr_vl_1_5: ModelManifest,
}

#[derive(Debug, Deserialize)]
struct ModelManifest {
    repository: String,
    revision: String,
    license: String,
    parameters: String,
    model_file: String,
    model_bytes: u64,
    model_sha256: String,
    mmproj_file: String,
    mmproj_bytes: u64,
    mmproj_sha256: String,
}

/// A failure while installing or running the local OCR model.
#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("the local OCR model is not installed")]
    ModelMissing,
    #[error("the local OCR runtime is not installed")]
    RuntimeMissing,
    #[error("local OCR download was cancelled")]
    Cancelled,
    #[error("local OCR download failed: {0}")]
    Download(String),
    #[error("local OCR checksum did not match for {file}")]
    Checksum { file: String },
    #[error("could not access local OCR files at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the local OCR runtime failed: {0}")]
    Runtime(String),
    #[error("the local OCR model returned no text")]
    EmptyResponse,
    #[error("the local OCR response was invalid: {0}")]
    InvalidResponse(String),
    #[error("the local OCR manifest is invalid: {0}")]
    Manifest(String),
}

/// Installation state for the local handwritten-text model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OcrModelStatus {
    Missing,
    /// Fraction downloaded, in the range `0.0..=1.0`.
    Downloading(f32),
    Installed {
        bytes: u64,
    },
}

pub(crate) struct Download {
    progress: std::sync::Arc<AtomicU64>,
    cancel: std::sync::Arc<AtomicBool>,
}

impl Download {
    pub(crate) fn new() -> Self {
        let progress = std::sync::Arc::new(AtomicU64::new(0));
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        Self { progress, cancel }
    }

    pub(crate) fn status(&self) -> OcrModelStatus {
        OcrModelStatus::Downloading(self.progress.load(Ordering::Relaxed) as f32 / 1_000_000.0)
    }

    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub(crate) fn worker_handles(&self) -> (std::sync::Arc<AtomicU64>, std::sync::Arc<AtomicBool>) {
        (
            std::sync::Arc::clone(&self.progress),
            std::sync::Arc::clone(&self.cancel),
        )
    }
}

impl Drop for Download {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn manifest() -> Result<&'static Manifest, OcrError> {
    MANIFEST
        .get_or_init(|| toml::from_str(MANIFEST_TEXT).map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| OcrError::Manifest(error.clone()))
}

fn model_dir(paths: &AppPaths) -> PathBuf {
    paths.models_dir().join(MODEL_ID)
}

fn marker(manifest: &ModelManifest) -> String {
    format!("{}\n{}\n", manifest.model_sha256, manifest.mmproj_sha256)
}

pub(crate) fn model_status(paths: &AppPaths) -> Result<OcrModelStatus, OcrError> {
    let manifest = manifest()?;
    let model = &manifest.ocr.paddleocr_vl_1_5;
    let dir = model_dir(paths);
    let marker_matches =
        fs::read_to_string(dir.join(VERIFIED_FILE)).is_ok_and(|value| value == marker(model));
    Ok(installed_status(
        marker_matches,
        file_size(&dir.join(&model.model_file)),
        file_size(&dir.join(&model.mmproj_file)),
        model,
    ))
}

fn file_size(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
}

fn installed_status(
    marker_matches: bool,
    model_bytes: Option<u64>,
    mmproj_bytes: Option<u64>,
    model: &ModelManifest,
) -> OcrModelStatus {
    if marker_matches
        && model_bytes == Some(model.model_bytes)
        && mmproj_bytes == Some(model.mmproj_bytes)
    {
        OcrModelStatus::Installed {
            bytes: model.model_bytes + model.mmproj_bytes,
        }
    } else {
        OcrModelStatus::Missing
    }
}

/// Downloads what local OCR still lacks here: the model, and on a computer
/// with a dedicated NVIDIA card the official CUDA build of llama-server.
/// A CUDA build that fails to install next to a new model is only logged:
/// the bundled Vulkan or CPU build still reads the pages.
pub(crate) fn download_model(
    paths: &AppPaths,
    progress: &dyn Fn(u64, u64),
    cancel: &AtomicBool,
) -> Result<(), OcrError> {
    let manifest = manifest()?;
    let model = &manifest.ocr.paddleocr_vl_1_5;
    let need_model = model_status(paths)? == OcrModelStatus::Missing;
    let cuda = missing_cuda_build(paths, manifest);
    let model_bytes = if need_model {
        model.model_bytes + model.mmproj_bytes
    } else {
        0
    };
    let total = model_bytes + cuda.map_or(0, CudaBuild::total_bytes);
    let root = paths.models_dir();
    fs::create_dir_all(&root).map_err(|source| OcrError::Io {
        path: root.clone(),
        source,
    })?;
    let mut fetcher = Fetcher::new(total, progress, cancel);
    let mut done = 0;
    if need_model {
        done = install_model_files(paths, model, &mut fetcher)?;
    }
    if let Some(build) = cuda {
        match install_cuda(paths, manifest, build, &mut fetcher, done) {
            Ok(()) => {}
            Err(error) if need_model && !matches!(error, OcrError::Cancelled) => {
                tracing::warn!(%error, "CUDA OCR runtime not installed; Vulkan or CPU will run");
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn install_model_files(
    paths: &AppPaths,
    model: &ModelManifest,
    fetcher: &mut Fetcher<'_>,
) -> Result<u64, OcrError> {
    let root = paths.models_dir();
    let staging =
        tempfile::tempdir_in(&root).map_err(|source| OcrError::Io { path: root, source })?;
    let mut done = 0;
    for (name, bytes, checksum) in [
        (&model.model_file, model.model_bytes, &model.model_sha256),
        (&model.mmproj_file, model.mmproj_bytes, &model.mmproj_sha256),
    ] {
        let url = format!("{}/resolve/{}/{}", model.repository, model.revision, name);
        done = fetcher.fetch(&url, &staging.path().join(name), bytes, checksum, done)?;
    }
    if fetcher.cancel.load(Ordering::Relaxed) {
        return Err(OcrError::Cancelled);
    }
    write_marker(staging.path(), &marker(model))?;
    replace_dir(staging.path(), &model_dir(paths))?;
    Ok(done)
}

/// Downloads both CUDA archives, unpacks llama-server and puts the CUDA
/// libraries next to it, where it looks for them.
fn install_cuda(
    paths: &AppPaths,
    manifest: &Manifest,
    build: &CudaBuild,
    fetcher: &mut Fetcher<'_>,
    mut done: u64,
) -> Result<(), OcrError> {
    let root = paths.models_dir();
    let staging = tempfile::tempdir_in(&root).map_err(|source| OcrError::Io {
        path: root.clone(),
        source,
    })?;
    let base = format!(
        "{}/releases/download/{}",
        manifest.llama_cpp.repository, manifest.llama_cpp.release
    );
    let server_archive = staging.path().join(&build.file);
    let libs_archive = staging.path().join(&build.runtime_file);
    done = fetcher.fetch(
        &format!("{base}/{}", build.file),
        &server_archive,
        build.bytes,
        &build.sha256,
        done,
    )?;
    fetcher.fetch(
        &format!("{base}/{}", build.runtime_file),
        &libs_archive,
        build.runtime_bytes,
        &build.runtime_sha256,
        done,
    )?;
    let runtime = staging.path().join("runtime");
    let libs = staging.path().join("libs");
    unpack(&server_archive, &runtime, fetcher.cancel)?;
    unpack(&libs_archive, &libs, fetcher.cancel)?;
    let server = find_server(&runtime)
        .ok_or_else(|| OcrError::Runtime("the CUDA build has no llama-server".into()))?;
    let server_dir = server.parent().unwrap_or(&runtime).to_path_buf();
    move_files_into(&libs, &server_dir)?;
    write_marker(&runtime, &build.marker())?;
    replace_dir(&runtime, &cuda_dir(paths, manifest))
}

fn unpack(archive: &Path, dest: &Path, cancel: &AtomicBool) -> Result<(), OcrError> {
    kg_audio::unpack_archive(archive, dest, cancel).map_err(|error| match error {
        kg_audio::AudioError::Cancelled => OcrError::Cancelled,
        other => OcrError::Runtime(format!("could not unpack {}: {other}", archive.display())),
    })
}

/// Moves every file under `from` (any depth) into `dest`, keeping files
/// already there.
fn move_files_into(from: &Path, dest: &Path) -> Result<(), OcrError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| OcrError::Io { path, source }
    };
    for entry in fs::read_dir(from).map_err(io(from))? {
        let path = entry.map_err(io(from))?.path();
        let kind = fs::symlink_metadata(&path).map_err(io(&path))?.file_type();
        if kind.is_dir() {
            move_files_into(&path, dest)?;
        } else if let Some(name) = path.file_name() {
            let target = dest.join(name);
            if !target.exists() {
                fs::rename(&path, &target).map_err(io(&target))?;
            }
        }
    }
    Ok(())
}

fn write_marker(dir: &Path, text: &str) -> Result<(), OcrError> {
    let path = dir.join(VERIFIED_FILE);
    fs::write(&path, text).map_err(|source| OcrError::Io { path, source })
}

fn replace_dir(from: &Path, to: &Path) -> Result<(), OcrError> {
    if to.exists() {
        fs::remove_dir_all(to).map_err(|source| OcrError::Io {
            path: to.to_path_buf(),
            source,
        })?;
    }
    fs::rename(from, to).map_err(|source| OcrError::Io {
        path: to.to_path_buf(),
        source,
    })
}

/// The platform name of the CUDA builds in `local-ai.toml`.
fn platform_key() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("windows-x86_64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x86_64")
    } else {
        None
    }
}

/// Whether this computer runs OCR on CUDA: a dedicated NVIDIA card with its
/// driver installed.
fn has_nvidia_card() -> bool {
    kg_audio::SpeechHardware::detect().dedicated_gpu == Some(kg_audio::GpuVendor::Nvidia)
        && kg_audio::nvidia_driver_present()
}

/// The CUDA build this computer should have, while it is not installed.
fn missing_cuda_build<'m>(paths: &AppPaths, manifest: &'m Manifest) -> Option<&'m CudaBuild> {
    if !has_nvidia_card() || cuda_server_executable(paths).is_some() {
        return None;
    }
    manifest.llama_cpp.cuda.get(platform_key()?)
}

fn cuda_dir(paths: &AppPaths, manifest: &Manifest) -> PathBuf {
    paths
        .models_dir()
        .join(format!("llama-cuda-{}", manifest.llama_cpp.release))
}

/// The installed and verified CUDA llama-server, if any.
fn cuda_server_executable(paths: &AppPaths) -> Option<PathBuf> {
    let manifest = manifest().ok()?;
    let build = manifest.llama_cpp.cuda.get(platform_key()?)?;
    let dir = cuda_dir(paths, manifest);
    let verified = fs::read_to_string(dir.join(VERIFIED_FILE)).ok()?;
    if verified != build.marker() {
        return None;
    }
    find_server(&dir)
}

/// llama-server in `dir` or a folder below it (the Linux archives keep their
/// files in one top folder).
fn find_server(dir: &Path) -> Option<PathBuf> {
    let direct = dir.join(SERVER_NAME);
    if direct.is_file() {
        return Some(direct);
    }
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .find_map(|path| find_server(&path))
}

struct Fetcher<'a> {
    agent: ureq::Agent,
    expected: u64,
    progress: &'a dyn Fn(u64, u64),
    cancel: &'a AtomicBool,
}

impl<'a> Fetcher<'a> {
    fn new(expected: u64, progress: &'a dyn Fn(u64, u64), cancel: &'a AtomicBool) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_body(Some(Duration::from_secs(3600)))
            .build()
            .into();
        Self {
            agent,
            expected,
            progress,
            cancel,
        }
    }

    fn fetch(
        &mut self,
        url: &str,
        path: &Path,
        expected_bytes: u64,
        expected_sha256: &str,
        mut done: u64,
    ) -> Result<u64, OcrError> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(OcrError::Cancelled);
        }
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|error| OcrError::Download(error.to_string()))?;
        let mut reader = response.into_body().into_reader();
        let mut file = fs::File::create(path).map_err(|source| OcrError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut hasher = Sha256::new();
        let mut received = 0_u64;
        let mut buffer = vec![0_u8; 256 * 1024];
        loop {
            if self.cancel.load(Ordering::Relaxed) {
                drop(file);
                let _ = fs::remove_file(path);
                return Err(OcrError::Cancelled);
            }
            let count = reader
                .read(&mut buffer)
                .map_err(|error| OcrError::Download(error.to_string()))?;
            if count == 0 {
                break;
            }
            received += count as u64;
            if received > expected_bytes {
                return Err(OcrError::Download(format!(
                    "unexpected size for {}: more than {expected_bytes} bytes",
                    path.display()
                )));
            }
            done += count as u64;
            hasher.update(&buffer[..count]);
            file.write_all(&buffer[..count])
                .map_err(|source| OcrError::Io {
                    path: path.to_path_buf(),
                    source,
                })?;
            (self.progress)(done, self.expected);
        }
        file.flush().map_err(|source| OcrError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        file.sync_all().map_err(|source| OcrError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        drop(file);
        if received != expected_bytes {
            return Err(OcrError::Download(format!(
                "unexpected size for {}: {received} bytes",
                path.display()
            )));
        }
        let actual = format!("{:x}", hasher.finalize());
        if actual != expected_sha256 {
            return Err(OcrError::Checksum {
                file: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            });
        }
        Ok(done)
    }
}

pub(crate) struct Server {
    child: Child,
    agent: ureq::Agent,
    endpoint: String,
    api_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeBackend {
    Cuda,
    Vulkan,
    Cpu,
}

impl RuntimeBackend {
    fn name(self) -> &'static str {
        match self {
            Self::Cuda => "CUDA",
            Self::Vulkan => "Vulkan",
            Self::Cpu => "CPU",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeCandidate {
    executable: PathBuf,
    backend: RuntimeBackend,
    device: Option<String>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Server")
            .field("pid", &self.child.id())
            .finish_non_exhaustive()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    pub(crate) fn start(paths: &AppPaths) -> Result<Self, OcrError> {
        let manifest = manifest()?;
        let model = &manifest.ocr.paddleocr_vl_1_5;
        if model_status(paths)? == OcrModelStatus::Missing {
            return Err(OcrError::ModelMissing);
        }
        let candidates = runtime_candidates(paths);
        start_with_fallback(&candidates, |candidate| {
            Self::start_with_runtime(paths, model, manifest, candidate)
        })
    }

    fn start_with_runtime(
        paths: &AppPaths,
        model: &ModelManifest,
        manifest: &Manifest,
        candidate: &RuntimeCandidate,
    ) -> Result<Self, OcrError> {
        tracing::info!(
            runtime = %manifest.llama_cpp.version,
            commit = %manifest.llama_cpp.commit,
            repository = %manifest.llama_cpp.repository,
            runtime_license = %manifest.llama_cpp.license,
            backend = ?candidate.backend,
            device = candidate.device.as_deref(),
            model = %model.parameters,
            model_license = %model.license,
            "starting local manuscript OCR"
        );
        let executable = &candidate.executable;
        let model_path = model_dir(paths).join(&model.model_file);
        let mmproj_path = model_dir(paths).join(&model.mmproj_file);
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|source| OcrError::Io {
            path: PathBuf::from("127.0.0.1"),
            source,
        })?;
        let port = listener
            .local_addr()
            .map_err(|source| OcrError::Io {
                path: PathBuf::from("127.0.0.1"),
                source,
            })?
            .port();
        drop(listener);
        let api_key = format!(
            "kg-ocr-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        );
        let mut command = Command::new(executable);
        command
            .arg("--model")
            .arg(model_path)
            .arg("--mmproj")
            .arg(mmproj_path)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--api-key")
            .arg(&api_key)
            .arg("--temp")
            .arg("0")
            .arg("--ctx-size")
            .arg("4096")
            .arg("--n-gpu-layers")
            .arg(match candidate.backend {
                RuntimeBackend::Cuda | RuntimeBackend::Vulkan => "99",
                RuntimeBackend::Cpu => "0",
            });
        // The Linux CUDA build loads its libraries from its own folder.
        if candidate.backend == RuntimeBackend::Cuda
            && cfg!(target_os = "linux")
            && let Some(dir) = executable.parent()
        {
            let mut search = vec![dir.to_path_buf()];
            if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH") {
                search.extend(std::env::split_paths(&existing));
            }
            if let Ok(joined) = std::env::join_paths(search) {
                command.env("LD_LIBRARY_PATH", joined);
            }
        }
        if let Some(device) = &candidate.device {
            command.arg("--device").arg(device);
        } else {
            command.arg("--device").arg("none");
        }
        command
            .arg("--threads")
            .arg(
                std::thread::available_parallelism()
                    .map(usize::from)
                    .unwrap_or(2)
                    .clamp(1, 4)
                    .to_string(),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let child = command.spawn().map_err(|error| {
            OcrError::Runtime(format!("could not start {}: {error}", executable.display()))
        })?;
        crate::child_job::tie_to_app(&child);
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(2)))
            .timeout_recv_body(Some(Duration::from_secs(600)))
            .http_status_as_error(false)
            .build()
            .into();
        let endpoint = format!("http://127.0.0.1:{port}");
        let mut server = Self {
            child,
            agent,
            endpoint,
            api_key,
        };
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        loop {
            if let Some(status) = server.child.try_wait().map_err(|error| {
                OcrError::Runtime(format!("could not check local OCR runtime: {error}"))
            })? {
                return Err(OcrError::Runtime(format!(
                    "local OCR runtime exited with {status}"
                )));
            }
            // 503 while the model loads; only 200 means it can answer.
            if server
                .agent
                .get(&format!("{}/health", server.endpoint))
                .call()
                .is_ok_and(|response| response.status() == 200)
            {
                return Ok(server);
            }
            if Instant::now() >= deadline {
                return Err(OcrError::Runtime(
                    "local OCR runtime did not become ready in time".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    pub(crate) fn transcribe(&self, image: &[u8], mime: &str) -> Result<String, OcrError> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(image);
        let request = serde_json::json!({
            "model": MODEL_ID,
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": "OCR:" },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": format!("data:{mime};base64,{encoded}"),
                            "detail": "high"
                        }
                    }
                ]
            }],
            "max_tokens": 16_000,
            "temperature": 0.0
        });
        let response = self
            .agent
            .post(&format!("{}{COMPLETIONS_PATH}", self.endpoint))
            .header("Authorization", &format!("Bearer {}", self.api_key))
            .send_json(&request)
            .map_err(|error| OcrError::Runtime(error.to_string()))?;
        let body = response
            .into_body()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_string()
            .map_err(|error| OcrError::InvalidResponse(error.to_string()))?;
        response_text(&body)
    }
}

#[derive(Debug, Deserialize)]
struct CompletionResponse {
    choices: Vec<CompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct CompletionChoice {
    message: CompletionMessage,
}

#[derive(Debug, Deserialize)]
struct CompletionMessage {
    #[serde(default)]
    content: String,
}

fn response_text(body: &str) -> Result<String, OcrError> {
    if let Ok(serde_json::Value::Object(fields)) = serde_json::from_str(body)
        && let Some(error) = fields.get("error")
    {
        let message = error["message"].as_str().unwrap_or_default();
        return Err(OcrError::Runtime(format!(
            "local OCR runtime refused: {message}"
        )));
    }
    let parsed: CompletionResponse =
        serde_json::from_str(body).map_err(|error| OcrError::InvalidResponse(error.to_string()))?;
    let text = parsed
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .unwrap_or_default();
    if text.trim().is_empty() {
        return Err(OcrError::EmptyResponse);
    }
    Ok(text)
}

pub(crate) fn ensure_server(
    paths: &AppPaths,
    server: &Mutex<Option<Server>>,
) -> Result<(), OcrError> {
    let mut server = server
        .lock()
        .map_err(|_| OcrError::Runtime("local OCR server state was poisoned".into()))?;
    ensure_server_slot(&mut server, || Server::start(paths))
}

pub(crate) fn transcribe(
    paths: &AppPaths,
    server: &Mutex<Option<Server>>,
    image: &[u8],
    mime: &str,
) -> Result<String, OcrError> {
    let mut server = server
        .lock()
        .map_err(|_| OcrError::Runtime("local OCR server state was poisoned".into()))?;
    ensure_server_slot(&mut server, || Server::start(paths))?;
    server
        .as_ref()
        .ok_or(OcrError::RuntimeMissing)?
        .transcribe(image, mime)
}

fn ensure_server_slot<T>(
    server: &mut Option<T>,
    start: impl FnOnce() -> Result<T, OcrError>,
) -> Result<(), OcrError> {
    if server.is_none() {
        *server = Some(start()?);
    }
    Ok(())
}

impl Engine {
    /// The effective OCR backend; Copilot always uses local OCR.
    pub fn manuscript_ocr(&self) -> crate::ManuscriptOcr {
        if self.is_copilot_provider() {
            crate::ManuscriptOcr::Local
        } else {
            self.settings.manuscript_ocr
        }
    }

    /// Download status for the pinned PaddleOCR-VL model.
    pub fn ocr_model_status(&self) -> Result<OcrModelStatus, OcrError> {
        if let Some(download) = &self.ocr_download {
            return Ok(download.status());
        }
        model_status(&self.paths)
    }

    /// Whether this installation includes or can find `llama-server`.
    pub fn ocr_runtime_available(&self) -> bool {
        cpu_server_executable().is_some()
            || vulkan_server_executable().is_some()
            || cuda_server_executable(&self.paths).is_some()
    }

    /// Bytes the local OCR download takes here: the model when missing, plus
    /// the CUDA build on a computer with a dedicated NVIDIA card.
    pub fn ocr_download_bytes(&self) -> u64 {
        let Ok(manifest) = manifest() else {
            return 0;
        };
        let model = &manifest.ocr.paddleocr_vl_1_5;
        let model_bytes = match model_status(&self.paths) {
            Ok(OcrModelStatus::Installed { .. }) => 0,
            _ => model.model_bytes + model.mmproj_bytes,
        };
        model_bytes + self.ocr_cuda_download_bytes()
    }

    /// Bytes of the CUDA build still to download; zero without a dedicated
    /// NVIDIA card or once it is installed.
    pub fn ocr_cuda_download_bytes(&self) -> u64 {
        manifest()
            .ok()
            .and_then(|manifest| missing_cuda_build(&self.paths, manifest))
            .map_or(0, CudaBuild::total_bytes)
    }

    /// Whether the local OCR model server is starting in the background.
    pub fn ocr_server_starting(&self) -> bool {
        self.ocr_server_starting
    }

    /// Stops the local OCR server on exit. A worker still starting it holds
    /// the lock, so this waits for it and stops what it started; otherwise the
    /// worker would store a running server after the app is gone.
    pub(crate) fn stop_ocr_server(&mut self) {
        let server = match self.ocr_server.lock() {
            Ok(mut server) => server.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        drop(server);
    }

    pub(crate) fn start_local_ocr_server(&mut self) {
        if self.manuscript_ocr() != crate::ManuscriptOcr::Local
            || self.ocr_server_starting
            || !self.ocr_runtime_available()
        {
            return;
        }
        match self.ocr_model_status() {
            Ok(OcrModelStatus::Installed { .. }) => {}
            Ok(OcrModelStatus::Missing | OcrModelStatus::Downloading(_)) => return,
            Err(error) => {
                self.notices.push(crate::Notice::Error(error.into()));
                return;
            }
        }
        match self.ocr_server.lock() {
            Ok(server) if server.is_some() => return,
            Ok(_) => {}
            Err(_) => {
                self.notices.push(crate::Notice::Error(
                    OcrError::Runtime("local OCR server state was poisoned".into()).into(),
                ));
                return;
            }
        }

        self.ocr_server_starting = true;
        let paths = self.paths.clone();
        let server = Arc::clone(&self.ocr_server);
        if !self.spawn(move || Event::OcrServerStarted(ensure_server(&paths, &server))) {
            self.ocr_server_starting = false;
            self.notices.push(crate::Notice::Error(
                OcrError::Runtime("could not start the local OCR worker".into()).into(),
            ));
        }
    }

    /// Whether the selected OCR backend is ready to read handwritten pages.
    pub fn manuscript_ocr_available(&self) -> Result<bool, OcrError> {
        match self.manuscript_ocr() {
            crate::ManuscriptOcr::Local => Ok(self.ocr_runtime_available()
                && matches!(self.ocr_model_status()?, OcrModelStatus::Installed { .. })),
            crate::ManuscriptOcr::ExternalVision => Ok(self.vision_target().is_some()),
        }
    }

    fn is_copilot_provider(&self) -> bool {
        crate::ai_provider(&self.settings.ai_provider)
            .or_else(|| crate::provider_for_url(&self.settings.ai.base_url))
            .is_some_and(|provider| provider.id == "copilot")
    }

    pub(crate) fn install_ocr_model(&mut self) -> Result<(), crate::EngineError> {
        if self.ocr_download.is_some()
            || (matches!(model_status(&self.paths)?, OcrModelStatus::Installed { .. })
                && self.ocr_cuda_download_bytes() == 0)
        {
            return Ok(());
        }
        let download = Download::new();
        let (progress, cancel) = download.worker_handles();
        self.ocr_download = Some(download);
        let paths = self.paths.clone();
        let waker = self.waker.clone();
        self.spawn(move || {
            let report = |done: u64, total: u64| {
                let fraction = (done as f64 / total.max(1) as f64).min(1.0);
                let completed = (fraction * 1_000_000.0) as u64;
                let previous = progress.swap(completed, Ordering::Relaxed);
                if completed / 10_000 != previous / 10_000 {
                    waker();
                }
            };
            Event::OcrModelDownloaded(download_model(&paths, &report, &cancel))
        });
        Ok(())
    }

    pub(crate) fn cancel_ocr_model_download(&mut self) {
        if let Some(download) = &self.ocr_download {
            download.cancel();
        }
    }
}

/// The builds to try, in order, chosen by the dedicated card: CUDA on NVIDIA,
/// Vulkan on AMD and Intel, the CPU build without one. A GPU build that fails
/// to start falls back to the next one and finally to the CPU build.
fn runtime_candidates(paths: &AppPaths) -> Vec<RuntimeCandidate> {
    let has_runtime_override = std::env::var_os("KG_LLAMA_SERVER")
        .map(PathBuf::from)
        .is_some_and(|path| path.is_file());
    if has_runtime_override {
        return ordered_candidates(None, None, cpu_server_executable());
    }
    let card = kg_audio::SpeechHardware::detect().dedicated_gpu;
    let cuda = has_nvidia_card()
        .then(|| cuda_server_executable(paths))
        .flatten();
    let vulkan = card.and_then(|card| {
        let executable = vulkan_server_executable()?;
        match detect_vulkan_device(&executable, card) {
            Ok(Some(device)) => Some((executable, device)),
            Ok(None) => {
                tracing::debug!(?card, "no Vulkan device for the dedicated card");
                None
            }
            Err(error) => {
                tracing::warn!(
                    path = %executable.display(),
                    %error,
                    "could not probe the Vulkan OCR runtime; using CPU if available"
                );
                None
            }
        }
    });
    ordered_candidates(cuda, vulkan, cpu_server_executable())
}

fn ordered_candidates(
    cuda: Option<PathBuf>,
    vulkan: Option<(PathBuf, String)>,
    cpu: Option<PathBuf>,
) -> Vec<RuntimeCandidate> {
    let mut candidates = Vec::with_capacity(3);
    if let Some(executable) = cuda {
        candidates.push(RuntimeCandidate {
            executable,
            backend: RuntimeBackend::Cuda,
            device: Some("CUDA0".to_owned()),
        });
    }
    if let Some((executable, device)) = vulkan {
        candidates.push(RuntimeCandidate {
            executable,
            backend: RuntimeBackend::Vulkan,
            device: Some(device),
        });
    }
    if let Some(executable) = cpu {
        candidates.push(RuntimeCandidate {
            executable,
            backend: RuntimeBackend::Cpu,
            device: None,
        });
    }
    candidates
}

fn start_with_fallback<T>(
    candidates: &[RuntimeCandidate],
    mut start: impl FnMut(&RuntimeCandidate) -> Result<T, OcrError>,
) -> Result<T, OcrError> {
    let mut failures = Vec::new();
    let mut last = None;
    for candidate in candidates {
        match start(candidate) {
            Ok(server) => return Ok(server),
            Err(error) => {
                let backend = candidate.backend.name();
                tracing::warn!(%error, backend, "local OCR runtime failed to start");
                failures.push(format!("{backend} startup failed ({error})"));
                last = Some(error);
            }
        }
    }
    match last {
        None => Err(OcrError::RuntimeMissing),
        Some(error) if failures.len() == 1 => Err(error),
        Some(_) => Err(OcrError::Runtime(failures.join("; "))),
    }
}

fn detect_vulkan_device(
    executable: &Path,
    card: kg_audio::GpuVendor,
) -> Result<Option<String>, String> {
    let output = Command::new(executable)
        .arg("--list-devices")
        .output()
        .map_err(|error| format!("could not run device probe: {error}"))?;
    if !output.status.success() {
        return Err(format!("device probe exited with {}", output.status));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(parse_vulkan_device(&format!("{stdout}\n{stderr}"), card))
}

/// The Vulkan device of the dedicated card, skipping software renderers and
/// integrated graphics (laptops often list both).
fn parse_vulkan_device(output: &str, card: kg_audio::GpuVendor) -> Option<String> {
    output.lines().find_map(|line| {
        let (device, name) = line.trim().split_once(':')?;
        let index = device.strip_prefix("Vulkan")?;
        if index.is_empty() || !index.chars().all(|character| character.is_ascii_digit()) {
            return None;
        }
        is_dedicated_device(name, card).then(|| device.to_owned())
    })
}

fn is_dedicated_device(name: &str, card: kg_audio::GpuVendor) -> bool {
    let name = name.trim().to_ascii_lowercase();
    let is_software_device = ["llvmpipe", "lavapipe", "swiftshader", "software rasterizer"]
        .iter()
        .any(|marker| name.contains(marker));
    if name.is_empty() || is_software_device {
        return false;
    }
    // Drivers append details: "AMD Radeon RX 5500 XT (RADV NAVI14)".
    let mut base = name.as_str();
    while base.ends_with(')')
        && let Some(open) = base.rfind('(')
    {
        base = base[..open].trim_end();
    }
    match card {
        kg_audio::GpuVendor::Nvidia => ["nvidia", "geforce", "quadro", "rtx"]
            .iter()
            .any(|marker| base.contains(marker)),
        // Radeon integrated graphics are named "... Graphics".
        kg_audio::GpuVendor::Amd => {
            (base.contains("amd") || base.contains("radeon")) && !base.ends_with("graphics")
        }
        // Intel's dedicated cards are the Arc series.
        kg_audio::GpuVendor::Intel => base.contains("intel") && base.contains("arc"),
    }
}

fn cpu_server_executable() -> Option<PathBuf> {
    let name = SERVER_NAME;
    if let Some(path) = std::env::var_os("KG_LLAMA_SERVER").map(PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }
    if let Some(path) = bundled_runtime_dirs()
        .into_iter()
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
    {
        return Some(path);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|path| path.join(name))
            .find(|path| path.is_file())
    })
}

fn vulkan_server_executable() -> Option<PathBuf> {
    let name = SERVER_NAME;
    bundled_runtime_dirs()
        .into_iter()
        .map(|directory| directory.join("vulkan").join(name))
        .find(|path| path.is_file())
}

fn bundled_runtime_dirs() -> Vec<PathBuf> {
    let Ok(executable) = std::env::current_exe() else {
        return Vec::new();
    };
    runtime_search_dirs(&executable, cfg!(debug_assertions))
}

fn runtime_search_dirs(executable: &Path, debug_build: bool) -> Vec<PathBuf> {
    let Some(executable_dir) = executable.parent() else {
        return Vec::new();
    };
    let mut directories = vec![executable_dir.join("local-runtime")];
    let target_dir = if executable_dir
        .file_name()
        .is_some_and(|name| name == "deps")
    {
        executable_dir.parent().and_then(Path::parent)
    } else {
        executable_dir.parent()
    };
    if debug_build && let Some(target_dir) = target_dir {
        directories.push(target_dir.join("release").join("local-runtime"));
    }
    directories
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_runtime_lookup_falls_back_to_the_packaged_release_runtime() {
        let executable = Path::new("repo")
            .join("target")
            .join("debug")
            .join("know-giver.exe");
        let directories = runtime_search_dirs(&executable, true);
        assert_eq!(
            directories,
            [
                Path::new("repo")
                    .join("target")
                    .join("debug")
                    .join("local-runtime"),
                Path::new("repo")
                    .join("target")
                    .join("release")
                    .join("local-runtime"),
            ]
        );
    }

    #[test]
    fn test_binary_runtime_lookup_reaches_the_workspace_release_runtime() {
        let executable = Path::new("repo")
            .join("target")
            .join("debug")
            .join("deps")
            .join("kg_engine.exe");
        let directories = runtime_search_dirs(&executable, true);
        assert_eq!(
            directories,
            [
                Path::new("repo")
                    .join("target")
                    .join("debug")
                    .join("deps")
                    .join("local-runtime"),
                Path::new("repo")
                    .join("target")
                    .join("release")
                    .join("local-runtime"),
            ]
        );
    }

    #[test]
    fn local_ocr_server_is_started_once_and_reused() {
        let mut server = None;
        let mut starts = 0;
        for _ in 0..2 {
            ensure_server_slot(&mut server, || {
                starts += 1;
                Ok::<_, OcrError>(starts)
            })
            .unwrap();
        }
        assert_eq!(starts, 1);
        assert_eq!(server, Some(1));
    }

    #[test]
    #[ignore = "requires the installed OCR model and bundled llama-server"]
    fn local_ocr_server_starts_in_background_when_local_ocr_is_enabled() {
        let paths = AppPaths::detect();
        assert!(matches!(
            model_status(&paths),
            Ok(OcrModelStatus::Installed { .. })
        ));

        let mut engine = Engine::in_memory(paths, crate::AiConfig::default()).unwrap();
        engine.set_waker(Arc::new(|| {}));
        let mut settings = engine.settings().clone();
        settings.manuscript_ocr = crate::ManuscriptOcr::Local;
        engine
            .handle(crate::Command::SaveSettings(settings))
            .unwrap();

        let deadline = Instant::now() + STARTUP_TIMEOUT + Duration::from_secs(10);
        while engine.ocr_server_starting() && Instant::now() < deadline {
            engine.poll();
            std::thread::sleep(Duration::from_millis(100));
        }
        engine.poll();

        assert!(
            !engine.ocr_server_starting(),
            "OCR server startup timed out"
        );
        let notices = engine.take_notices();
        assert!(
            engine
                .ocr_server
                .lock()
                .is_ok_and(|server| server.is_some()),
            "OCR server did not remain available after startup: {notices:?}"
        );
    }

    /// `KG_OCR_SAMPLE=page.jpg cargo test -p kg-engine local_ocr_reads -- --ignored --nocapture`
    #[test]
    #[ignore = "requires the installed OCR model, bundled llama-server and KG_OCR_SAMPLE"]
    fn local_ocr_reads_a_handwritten_page() {
        let sample = std::env::var("KG_OCR_SAMPLE").expect("KG_OCR_SAMPLE");
        let image = fs::read(&sample).unwrap();
        let paths = AppPaths::detect();
        let server = Mutex::new(None);
        let start = Instant::now();
        ensure_server(&paths, &server).unwrap();
        println!("server ready in {:.1}s", start.elapsed().as_secs_f32());
        let start = Instant::now();
        let text = transcribe(&paths, &server, &image, "image/jpeg").unwrap();
        println!(
            "read in {:.1}s, {} chars:\n{text}",
            start.elapsed().as_secs_f32(),
            text.chars().count()
        );
        assert!(text.chars().count() > 200, "{text}");
    }

    /// Installs the CUDA build next to the installed model and starts the OCR
    /// server on it. Needs a dedicated NVIDIA card; downloads about 590 MB once.
    /// `cargo test -p kg-engine cuda_runtime_installs -- --ignored --nocapture`
    #[test]
    #[ignore = "downloads the CUDA build; needs an NVIDIA card and the installed OCR model"]
    fn cuda_runtime_installs_and_starts() {
        let paths = AppPaths::detect();
        let manifest = manifest().unwrap();
        assert!(has_nvidia_card(), "no dedicated NVIDIA card here");
        let start = Instant::now();
        download_model(&paths, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        let executable = cuda_server_executable(&paths).expect("CUDA build installed");
        println!(
            "installed in {:.0}s: {}",
            start.elapsed().as_secs_f32(),
            executable.display()
        );
        let candidate = RuntimeCandidate {
            executable,
            backend: RuntimeBackend::Cuda,
            device: Some("CUDA0".to_owned()),
        };
        let start = Instant::now();
        let server = Server::start_with_runtime(
            &paths,
            &manifest.ocr.paddleocr_vl_1_5,
            manifest,
            &candidate,
        )
        .unwrap();
        println!("CUDA server ready in {:.1}s", start.elapsed().as_secs_f32());
        assert_eq!(runtime_candidates(&paths)[0].backend, RuntimeBackend::Cuda);
        drop(server);
    }

    #[test]
    fn manifest_pins_the_official_model_and_runtime() {
        let manifest = manifest().unwrap();
        assert_eq!(
            manifest.llama_cpp.repository,
            "https://github.com/ggml-org/llama.cpp"
        );
        assert_eq!(manifest.llama_cpp.version, "v0.8.0");
        assert_eq!(
            manifest.llama_cpp.commit,
            "8642541b255bfb5dc39cd351a178be09013a55d6"
        );
        assert_eq!(manifest.llama_cpp.license, "MIT");
        assert_eq!(manifest.llama_cpp.release, "b11960");
        assert_eq!(
            manifest.ocr.paddleocr_vl_1_5.repository,
            "https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.5-GGUF"
        );
        assert_eq!(manifest.ocr.paddleocr_vl_1_5.revision.len(), 40);
        assert_eq!(manifest.ocr.paddleocr_vl_1_5.license, "Apache-2.0");
        assert_eq!(manifest.ocr.paddleocr_vl_1_5.model_bytes, 935_768_992);
        assert_eq!(manifest.ocr.paddleocr_vl_1_5.mmproj_bytes, 881_770_496);
    }

    #[test]
    fn model_install_status_requires_both_weights_and_verified_marker() {
        let directory = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(directory.path());
        assert_eq!(model_status(&paths).unwrap(), OcrModelStatus::Missing);
        let manifest = manifest().unwrap();
        let model = &manifest.ocr.paddleocr_vl_1_5;
        let dir = model_dir(&paths);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(&model.model_file), b"model").unwrap();
        fs::write(dir.join(&model.mmproj_file), b"mmproj").unwrap();
        fs::write(dir.join("verified.sha256"), marker(model)).unwrap();
        assert_eq!(model_status(&paths).unwrap(), OcrModelStatus::Missing);
    }

    #[test]
    fn installed_status_requires_exact_sizes_for_both_model_files() {
        let model = &manifest().unwrap().ocr.paddleocr_vl_1_5;
        let expected = OcrModelStatus::Installed {
            bytes: model.model_bytes + model.mmproj_bytes,
        };
        let cases = [
            (
                "valid marker and both complete files",
                true,
                Some(model.model_bytes),
                Some(model.mmproj_bytes),
                expected,
            ),
            (
                "invalid marker",
                false,
                Some(model.model_bytes),
                Some(model.mmproj_bytes),
                OcrModelStatus::Missing,
            ),
            (
                "missing model file",
                true,
                None,
                Some(model.mmproj_bytes),
                OcrModelStatus::Missing,
            ),
            (
                "truncated projection file",
                true,
                Some(model.model_bytes),
                Some(model.mmproj_bytes - 1),
                OcrModelStatus::Missing,
            ),
        ];

        for (case, marker_matches, model_bytes, mmproj_bytes, expected) in cases {
            assert_eq!(
                installed_status(marker_matches, model_bytes, mmproj_bytes, model),
                expected,
                "case: {case}"
            );
        }
    }

    #[test]
    fn local_ocr_response_returns_text_and_reports_empty_or_invalid_results() {
        assert_eq!(
            response_text(r#"{"choices":[{"message":{"content":"line one\nline two"}}]}"#).unwrap(),
            "line one\nline two"
        );
        assert!(matches!(
            response_text(r#"{"choices":[]}"#),
            Err(OcrError::EmptyResponse)
        ));
        assert!(matches!(
            response_text(r#"{"choices":[{"message":{"content":"  "}}]}"#),
            Err(OcrError::EmptyResponse)
        ));
        assert!(matches!(
            response_text("not json"),
            Err(OcrError::InvalidResponse(_))
        ));
    }

    #[test]
    fn device_probe_picks_the_dedicated_card_and_skips_integrated_and_software() {
        use kg_audio::GpuVendor::{Amd, Intel, Nvidia};
        let cases = [
            (
                "NVIDIA",
                "Available devices:\n  Vulkan0: NVIDIA GeForce RTX 4090\n  CPU: x86\n",
                Nvidia,
                Some("Vulkan0"),
            ),
            (
                "AMD",
                "Available devices:\n  Vulkan0: AMD Radeon RX 7900 XTX\n",
                Amd,
                Some("Vulkan0"),
            ),
            (
                "AMD on RADV",
                "Available devices:\n  Vulkan0: AMD Radeon RX 5500 XT (RADV NAVI14)\n",
                Amd,
                Some("Vulkan0"),
            ),
            (
                "RX 5500 on Linux, as llama.cpp lists it",
                "MESA-INTEL: warning: Haswell Vulkan support is incomplete\nAvailable devices:\n  Vulkan0: AMD Radeon RX 5500 (RADV NAVI14) (8192 MiB, 7879 MiB free)\n",
                Amd,
                Some("Vulkan0"),
            ),
            (
                "AMD laptop with integrated graphics first",
                "Available devices:\n  Vulkan0: AMD Radeon(TM) Graphics (RADV RENOIR)\n  Vulkan1: AMD Radeon RX 6600M (RADV NAVI23)\n",
                Amd,
                Some("Vulkan1"),
            ),
            (
                "Intel Arc after Intel UHD",
                "Available devices:\n  Vulkan0: Intel(R) UHD Graphics 770\n  Vulkan1: Intel(R) Arc(TM) A770 Graphics\n",
                Intel,
                Some("Vulkan1"),
            ),
            (
                "NVIDIA laptop with Intel graphics first",
                "Available devices:\n  Vulkan0: Intel(R) Iris(R) Xe Graphics\n  Vulkan1: NVIDIA GeForce RTX 3050 Laptop GPU\n",
                Nvidia,
                Some("Vulkan1"),
            ),
            (
                "only integrated graphics",
                "Available devices:\n  Vulkan0: AMD Radeon(TM) Graphics\n",
                Amd,
                None,
            ),
            ("CPU only", "Available devices:\n  CPU: x86\n", Nvidia, None),
            (
                "software renderer",
                "Available devices:\n  Vulkan0: llvmpipe (LLVM 17.0.6, 256 bits)\n",
                Amd,
                None,
            ),
            (
                "backend mention without device entry",
                "Vulkan backend is unavailable\n  CPU: x86\n",
                Intel,
                None,
            ),
        ];

        for (case, output, card, expected) in cases {
            assert_eq!(
                parse_vulkan_device(output, card).as_deref(),
                expected,
                "case: {case}"
            );
        }
    }

    #[test]
    fn runtime_candidates_go_cuda_then_vulkan_then_cpu() {
        let cuda_path = PathBuf::from("cuda/llama-server");
        let vulkan_path = PathBuf::from("vulkan/llama-server");
        let cpu_path = PathBuf::from("cpu/llama-server");
        let candidates = ordered_candidates(
            Some(cuda_path.clone()),
            Some((vulkan_path.clone(), "Vulkan0".to_owned())),
            Some(cpu_path.clone()),
        );

        assert_eq!(
            candidates,
            [
                RuntimeCandidate {
                    executable: cuda_path,
                    backend: RuntimeBackend::Cuda,
                    device: Some("CUDA0".to_owned()),
                },
                RuntimeCandidate {
                    executable: vulkan_path,
                    backend: RuntimeBackend::Vulkan,
                    device: Some("Vulkan0".to_owned()),
                },
                RuntimeCandidate {
                    executable: cpu_path,
                    backend: RuntimeBackend::Cpu,
                    device: None,
                },
            ]
        );
    }

    #[test]
    fn runtime_candidates_use_cpu_without_a_dedicated_card() {
        let cpu_path = PathBuf::from("cpu/llama-server");
        let candidates = ordered_candidates(None, None, Some(cpu_path.clone()));

        assert_eq!(
            candidates,
            [RuntimeCandidate {
                executable: cpu_path,
                backend: RuntimeBackend::Cpu,
                device: None,
            }]
        );
    }

    #[test]
    fn failed_gpu_startups_fall_back_down_to_the_cpu_runtime() {
        let candidates = ordered_candidates(
            Some(PathBuf::from("cuda/llama-server")),
            Some((PathBuf::from("vulkan/llama-server"), "Vulkan0".to_owned())),
            Some(PathBuf::from("cpu/llama-server")),
        );
        let mut attempted = Vec::new();
        let result: Result<&str, OcrError> = start_with_fallback(&candidates, |candidate| {
            attempted.push(candidate.backend);
            match candidate.backend {
                RuntimeBackend::Cuda => Err(OcrError::Runtime("driver too old".into())),
                RuntimeBackend::Vulkan => Err(OcrError::Runtime("driver unavailable".into())),
                RuntimeBackend::Cpu => Ok("cpu server"),
            }
        });

        assert_eq!(result.unwrap(), "cpu server");
        assert_eq!(
            attempted,
            [
                RuntimeBackend::Cuda,
                RuntimeBackend::Vulkan,
                RuntimeBackend::Cpu
            ]
        );
    }

    #[test]
    fn failed_cpu_fallback_reports_every_backend_error() {
        let candidates = ordered_candidates(
            Some(PathBuf::from("cuda/llama-server")),
            Some((PathBuf::from("vulkan/llama-server"), "Vulkan0".to_owned())),
            Some(PathBuf::from("cpu/llama-server")),
        );
        let result: Result<(), OcrError> = start_with_fallback(&candidates, |candidate| {
            Err(OcrError::Runtime(format!(
                "{} failed",
                candidate.backend.name()
            )))
        });

        assert!(matches!(
            result,
            Err(OcrError::Runtime(message))
                if message.contains("CUDA failed")
                    && message.contains("Vulkan failed")
                    && message.contains("CPU failed")
        ));
    }

    #[test]
    fn cuda_builds_are_pinned_for_windows_and_linux() {
        let manifest = manifest().unwrap();
        assert_eq!(manifest.llama_cpp.release, "b11960");
        for key in ["windows-x86_64", "linux-x86_64"] {
            let build = &manifest.llama_cpp.cuda[key];
            for (file, sha) in [
                (&build.file, &build.sha256),
                (&build.runtime_file, &build.runtime_sha256),
            ] {
                assert!(file.contains("cuda"), "{file}");
                assert_eq!(sha.len(), 64, "{file}");
                assert!(sha.chars().all(|c| c.is_ascii_hexdigit()), "{file}");
            }
            assert!(build.file.contains(&manifest.llama_cpp.release), "{key}");
            assert!(build.bytes > 100_000_000 && build.runtime_bytes > 100_000_000);
        }
    }

    #[test]
    fn cuda_libraries_land_next_to_llama_server() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = directory.path().join("runtime");
        let nested = runtime.join("llama-b11960");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join(SERVER_NAME), b"server").unwrap();
        let libs = directory.path().join("libs");
        fs::create_dir_all(libs.join("lib")).unwrap();
        fs::write(libs.join("cudart.dll"), b"a").unwrap();
        fs::write(libs.join("lib").join("cublas.dll"), b"b").unwrap();

        let server = find_server(&runtime).unwrap();
        assert_eq!(server, nested.join(SERVER_NAME));
        move_files_into(&libs, &nested).unwrap();
        assert!(nested.join("cudart.dll").is_file());
        assert!(nested.join("cublas.dll").is_file());
    }
}
