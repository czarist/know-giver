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
        fs::read_to_string(dir.join("verified.sha256")).is_ok_and(|value| value == marker(model));
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

pub(crate) fn download_model(
    paths: &AppPaths,
    progress: &dyn Fn(u64, u64),
    cancel: &AtomicBool,
) -> Result<(), OcrError> {
    let manifest = manifest()?;
    let model = &manifest.ocr.paddleocr_vl_1_5;
    let total = model.model_bytes + model.mmproj_bytes;
    let root = paths.models_dir();
    fs::create_dir_all(&root).map_err(|source| OcrError::Io {
        path: root.clone(),
        source,
    })?;
    let staging =
        tempfile::tempdir_in(&root).map_err(|source| OcrError::Io { path: root, source })?;
    let mut done = 0;
    let mut fetcher = Fetcher::new(total, progress, cancel);
    for (name, bytes, checksum) in [
        (&model.model_file, model.model_bytes, &model.model_sha256),
        (&model.mmproj_file, model.mmproj_bytes, &model.mmproj_sha256),
    ] {
        let url = format!("{}/resolve/{}/{}", model.repository, model.revision, name);
        done = fetcher.fetch(&url, &staging.path().join(name), bytes, checksum, done)?;
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(OcrError::Cancelled);
    }
    fs::write(staging.path().join("verified.sha256"), marker(model)).map_err(|source| {
        OcrError::Io {
            path: staging.path().join("verified.sha256"),
            source,
        }
    })?;
    let destination = model_dir(paths);
    if destination.exists() {
        fs::remove_dir_all(&destination).map_err(|source| OcrError::Io {
            path: destination.clone(),
            source,
        })?;
    }
    fs::rename(staging.path(), &destination).map_err(|source| OcrError::Io {
        path: destination,
        source,
    })?;
    Ok(())
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
    Vulkan,
    Cpu,
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
        let candidates = runtime_candidates();
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
                RuntimeBackend::Vulkan => "99",
                RuntimeBackend::Cpu => "0",
            });
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
            if server
                .agent
                .get(&format!("{}/health", server.endpoint))
                .call()
                .is_ok()
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
        cpu_server_executable().is_some() || vulkan_server_executable().is_some()
    }

    /// Whether the local OCR model server is starting in the background.
    pub fn ocr_server_starting(&self) -> bool {
        self.ocr_server_starting
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
            || matches!(model_status(&self.paths)?, OcrModelStatus::Installed { .. })
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

fn runtime_candidates() -> Vec<RuntimeCandidate> {
    let has_runtime_override = std::env::var_os("KG_LLAMA_SERVER")
        .map(PathBuf::from)
        .is_some_and(|path| path.is_file());
    let vulkan = (!has_runtime_override)
        .then(vulkan_server_executable)
        .flatten()
        .and_then(|executable| match detect_vulkan_device(&executable) {
            Ok(Some(device)) => Some((executable, device)),
            Ok(None) => {
                tracing::debug!(path = %executable.display(), "no hardware Vulkan device found");
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
        });
    ordered_candidates(vulkan, cpu_server_executable())
}

fn ordered_candidates(
    vulkan: Option<(PathBuf, String)>,
    cpu: Option<PathBuf>,
) -> Vec<RuntimeCandidate> {
    let mut candidates = Vec::with_capacity(2);
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
    let mut vulkan_error = None;
    for candidate in candidates {
        match start(candidate) {
            Ok(server) => return Ok(server),
            Err(error) if candidate.backend == RuntimeBackend::Vulkan => {
                tracing::warn!(%error, "Vulkan OCR startup failed; trying the CPU runtime");
                vulkan_error = Some(error);
            }
            Err(cpu_error) => {
                return Err(match vulkan_error {
                    Some(vulkan_error) => OcrError::Runtime(format!(
                        "Vulkan startup failed ({vulkan_error}); CPU fallback failed: {cpu_error}"
                    )),
                    None => cpu_error,
                });
            }
        }
    }
    match vulkan_error {
        Some(error) => Err(OcrError::Runtime(format!(
            "Vulkan startup failed and no CPU runtime is available: {error}"
        ))),
        None => Err(OcrError::RuntimeMissing),
    }
}

fn detect_vulkan_device(executable: &Path) -> Result<Option<String>, String> {
    let output = Command::new(executable)
        .arg("--list-devices")
        .output()
        .map_err(|error| format!("could not run device probe: {error}"))?;
    if !output.status.success() {
        return Err(format!("device probe exited with {}", output.status));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(parse_vulkan_device(&format!("{stdout}\n{stderr}")))
}

fn parse_vulkan_device(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (device, name) = line.trim().split_once(':')?;
        let index = device.strip_prefix("Vulkan")?;
        if index.is_empty() || !index.chars().all(|character| character.is_ascii_digit()) {
            return None;
        }
        let name = name.trim();
        let normalized_name = name.to_ascii_lowercase();
        let is_software_device = ["llvmpipe", "lavapipe", "swiftshader", "software rasterizer"]
            .iter()
            .any(|marker| normalized_name.contains(marker));
        (!name.is_empty() && !is_software_device).then(|| device.to_owned())
    })
}

fn cpu_server_executable() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
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
    let name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
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
    fn device_probe_selects_hardware_vulkan_devices_and_ignores_software_renderers() {
        let cases = [
            (
                "NVIDIA",
                "Available devices:\n  Vulkan0: NVIDIA GeForce RTX 4090\n  CPU: x86\n",
                Some("Vulkan0"),
            ),
            (
                "AMD",
                "Available devices:\n  Vulkan0: AMD Radeon RX 7900 XTX\n",
                Some("Vulkan0"),
            ),
            (
                "Intel",
                "Available devices:\n  Vulkan1: Intel Arc A770\n",
                Some("Vulkan1"),
            ),
            ("CPU only", "Available devices:\n  CPU: x86\n", None),
            (
                "software renderer",
                "Available devices:\n  Vulkan0: llvmpipe (LLVM 17.0.6, 256 bits)\n",
                None,
            ),
            (
                "backend mention without device entry",
                "Vulkan backend is unavailable\n  CPU: x86\n",
                None,
            ),
        ];

        for (case, output, expected) in cases {
            assert_eq!(
                parse_vulkan_device(output).as_deref(),
                expected,
                "case: {case}"
            );
        }
    }

    #[test]
    fn runtime_candidates_prefer_detected_vulkan_and_keep_cpu_fallback() {
        let vulkan_path = PathBuf::from("vulkan/llama-server");
        let cpu_path = PathBuf::from("cpu/llama-server");
        let candidates = ordered_candidates(
            Some((vulkan_path.clone(), "Vulkan0".to_owned())),
            Some(cpu_path.clone()),
        );

        assert_eq!(
            candidates,
            [
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
    fn runtime_candidates_use_cpu_when_no_vulkan_device_is_detected() {
        let cpu_path = PathBuf::from("cpu/llama-server");
        let candidates = ordered_candidates(None, Some(cpu_path.clone()));

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
    fn failed_vulkan_startup_retries_with_cpu_runtime() {
        let candidates = ordered_candidates(
            Some((PathBuf::from("vulkan/llama-server"), "Vulkan0".to_owned())),
            Some(PathBuf::from("cpu/llama-server")),
        );
        let mut attempted = Vec::new();
        let result: Result<&str, OcrError> = start_with_fallback(&candidates, |candidate| {
            attempted.push(candidate.backend);
            match candidate.backend {
                RuntimeBackend::Vulkan => Err(OcrError::Runtime("driver unavailable".into())),
                RuntimeBackend::Cpu => Ok("cpu server"),
            }
        });

        assert_eq!(result.unwrap(), "cpu server");
        assert_eq!(attempted, [RuntimeBackend::Vulkan, RuntimeBackend::Cpu]);
    }

    #[test]
    fn failed_cpu_fallback_reports_both_backend_errors() {
        let candidates = ordered_candidates(
            Some((PathBuf::from("vulkan/llama-server"), "Vulkan0".to_owned())),
            Some(PathBuf::from("cpu/llama-server")),
        );
        let result: Result<(), OcrError> = start_with_fallback(&candidates, |candidate| {
            Err(OcrError::Runtime(match candidate.backend {
                RuntimeBackend::Vulkan => "Vulkan failed".into(),
                RuntimeBackend::Cpu => "CPU failed".into(),
            }))
        });

        assert!(matches!(
            result,
            Err(OcrError::Runtime(message))
                if message.contains("Vulkan failed") && message.contains("CPU failed")
        ));
    }
}
