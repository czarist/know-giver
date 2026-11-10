//! Local model catalog: what can be downloaded, where it lives, and whether
//! it is installed.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::AudioError;

const HF: &str = "https://huggingface.co";
/// OpenVoiceOS "Dii" pt-BR voice (CC BY-NC-ND 4.0), pinned to a revision.
const DII_BASE: &str = "https://huggingface.co/OpenVoiceOS/pipertts_pt-BR_dii/resolve/38281189bdbc6a0346041925b56da6d71d6fba3d/dii_pt-BR";
const KOKORO_MODEL_DIR: &str = "kokoro-multi-lang-v1_0";
const KOKORO_MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-multi-lang-v1_0.tar.bz2";
const PARAKEET_DIR: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
const PARAKEET_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2";
const PARAKEET_FILES: [&str; 4] = [
    "encoder.int8.onnx",
    "decoder.int8.onnx",
    "joiner.int8.onnx",
    "tokens.txt",
];

/// An official, pinned build of a program the app runs, for one system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Package {
    url: &'static str,
    sha256: &'static str,
    /// Top folder inside the archive.
    dir: &'static str,
}

macro_rules! piper {
    ($file:literal, $sha256:literal) => {
        Package {
            url: concat!(
                "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/",
                $file
            ),
            sha256: $sha256,
            dir: "piper",
        }
    };
}

macro_rules! sherpa {
    ($dir:literal, $sha256:literal) => {
        Package {
            url: concat!(
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.12.20/",
                $dir,
                ".tar.bz2"
            ),
            sha256: $sha256,
            dir: $dir,
        }
    };
}

/// Piper 2023.11.14-2. Its macOS archives are broken (an Intel binary
/// without its libraries), so macOS has no Piper download.
fn piper_package() -> Option<Package> {
    if cfg!(windows) {
        // x64 build; Windows on Arm runs it through emulation.
        Some(piper!(
            "piper_windows_amd64.zip",
            "f3c58906402b24f3a96d92145f58acba6d86c9b5db896d207f78dc80811efcea"
        ))
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(piper!(
            "piper_linux_x86_64.tar.gz",
            "a50cb45f355b7af1f6d758c1b360717877ba0a398cc8cbe6d2a7a3a26e225992"
        ))
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some(piper!(
            "piper_linux_aarch64.tar.gz",
            "fea0fd2d87c54dbc7078d0f878289f404bd4d6eea6e7444a77835d1537ab88eb"
        ))
    } else {
        None
    }
}

/// sherpa-onnx v1.12.20 (Kokoro voices and Parakeet). The app loads its C
/// library, so the build must match the app's own architecture.
fn sherpa_package() -> Option<Package> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some(sherpa!(
            "sherpa-onnx-v1.12.20-win-x64-shared",
            "2aa9c11f13d6a20308afedcaa8c04a678a1aaebc206b7609e4df7d7910582f56"
        ))
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(sherpa!(
            "sherpa-onnx-v1.12.20-linux-x64-shared",
            "13a2c502f5ef373f514a5fc5c5ad03cc0798dcb564a6f8cbd54f8f6ce9e47462"
        ))
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some(sherpa!(
            "sherpa-onnx-v1.12.20-linux-aarch64-shared-cpu",
            "d8b90c7e92b153c64be8403fdb3e45180473a646f3ac4447f75737cf4fc67d69"
        ))
    } else if cfg!(target_os = "macos") {
        // Universal build: Apple silicon and Intel.
        Some(sherpa!(
            "sherpa-onnx-v1.12.20-osx-universal2-shared",
            "ee78b87028e42e461cd828ce4a4fdcaed602b659a811cd104c4219aecf0a0290"
        ))
    } else {
        None
    }
}

/// `local` when it exists; outside Windows, also a program the student
/// installed on `PATH`.
fn find_program(local: Option<PathBuf>, name: &str) -> Option<PathBuf> {
    if let Some(local) = local.filter(|path| path.is_file()) {
        return Some(local);
    }
    if cfg!(windows) {
        return None;
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(name))
            .find(|p| p.is_file())
    })
}

fn exe_name(program: &str) -> String {
    if cfg!(windows) {
        format!("{program}.exe")
    } else {
        program.to_owned()
    }
}

/// Multilingual Whisper checkpoints. Bigger is more accurate and slower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WhisperModel {
    Tiny,
    #[default]
    Base,
    Small,
}

impl WhisperModel {
    pub const ALL: [Self; 3] = [Self::Tiny, Self::Base, Self::Small];

    pub fn id(self) -> &'static str {
        match self {
            Self::Tiny => "whisper-tiny",
            Self::Base => "whisper-base",
            Self::Small => "whisper-small",
        }
    }

    fn approx_bytes(self) -> u64 {
        match self {
            Self::Tiny => 153_000_000,
            Self::Base => 293_000_000,
            Self::Small => 970_000_000,
        }
    }
}

/// Which speech recognizer the lessons use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SttEngine {
    /// Parakeet through the sherpa-onnx runtime: fast enough for live captions.
    #[default]
    Parakeet,
    Whisper,
}

/// Brazilian Portuguese Piper voices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PiperVoice {
    #[default]
    Faber,
    Cadu,
    Jeff,
    /// Female voice by OpenVoiceOS, fine-tuned for pt-BR.
    Dii,
}

impl PiperVoice {
    pub const ALL: [Self; 4] = [Self::Faber, Self::Cadu, Self::Jeff, Self::Dii];

    pub fn id(self) -> &'static str {
        match self {
            Self::Faber => "pt_BR-faber-medium",
            Self::Cadu => "pt_BR-cadu-medium",
            Self::Jeff => "pt_BR-jeff-medium",
            Self::Dii => "pt_BR-dii-ovos",
        }
    }

    /// Download URL without the `.onnx` / `.onnx.json` extension.
    fn download_base(self) -> String {
        let speaker = match self {
            Self::Faber => "faber",
            Self::Cadu => "cadu",
            Self::Jeff => "jeff",
            Self::Dii => return DII_BASE.to_owned(),
        };
        format!(
            "{HF}/rhasspy/piper-voices/resolve/main/pt/pt_BR/{speaker}/medium/{}",
            self.id()
        )
    }
}

/// Anything the model manager can install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelId {
    Whisper(WhisperModel),
    Voice(PiperVoice),
    /// The Piper executable itself.
    PiperRuntime,
    /// Pinned sherpa-onnx runtime; no native compilation in the app.
    KokoroRuntime,
    /// Shared Kokoro v1.0 model containing all three Portuguese voices.
    KokoroModel,
    /// NVIDIA Parakeet TDT 0.6B v3 speech recognition (needs the sherpa-onnx
    /// runtime, [`ModelId::KokoroRuntime`]).
    Parakeet,
}

impl ModelId {
    pub fn all() -> Vec<Self> {
        let mut all = vec![Self::Parakeet];
        all.extend(WhisperModel::ALL.into_iter().map(Self::Whisper));
        all.push(Self::PiperRuntime);
        all.extend(PiperVoice::ALL.into_iter().map(Self::Voice));
        all.extend([Self::KokoroRuntime, Self::KokoroModel]);
        all
    }

    /// Rough download size, for progress bars before the server tells us.
    pub fn approx_bytes(self) -> u64 {
        match self {
            Self::Whisper(m) => m.approx_bytes(),
            Self::Voice(_) => 63_500_000,
            Self::PiperRuntime if cfg!(target_os = "linux") => 26_000_000,
            Self::PiperRuntime => 22_000_000,
            Self::KokoroRuntime if cfg!(target_os = "macos") => 40_200_000,
            Self::KokoroRuntime => 26_700_000,
            Self::KokoroModel => 350_000_000,
            Self::Parakeet => 487_200_000,
        }
    }

    /// The app can download this on the current system. Programs without an
    /// official build here can still be found on `PATH` outside Windows.
    pub fn downloadable(self) -> bool {
        match self {
            Self::PiperRuntime => piper_package().is_some(),
            Self::KokoroRuntime => sherpa_package().is_some(),
            _ => true,
        }
    }
}

/// Where models are stored on disk, and how to fetch them.
#[derive(Debug, Clone)]
pub struct ModelStore {
    root: PathBuf,
}

impl ModelStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn whisper_dir(&self, model: WhisperModel) -> PathBuf {
        self.root.join(model.id())
    }

    /// `(model.onnx, model.onnx.json)` for a voice.
    pub fn voice_files(&self, voice: PiperVoice) -> (PathBuf, PathBuf) {
        let dir = self.root.join("voices");
        (
            dir.join(format!("{}.onnx", voice.id())),
            dir.join(format!("{}.onnx.json", voice.id())),
        )
    }

    fn piper_dir(&self) -> PathBuf {
        self.root.join("piper")
    }

    pub fn kokoro_dir(&self) -> PathBuf {
        self.root.join("kokoro").join(KOKORO_MODEL_DIR)
    }

    pub fn parakeet_dir(&self) -> PathBuf {
        self.root.join("parakeet").join(PARAKEET_DIR)
    }

    /// The downloaded sherpa-onnx runtime, or one installed on `PATH`
    /// outside Windows.
    pub fn kokoro_exe(&self) -> Option<PathBuf> {
        self.sherpa_exe("sherpa-onnx-offline-tts")
    }

    /// The sherpa-onnx speech recognition server used for Parakeet.
    pub fn asr_server_exe(&self) -> Option<PathBuf> {
        self.sherpa_exe("sherpa-onnx-offline-websocket-server")
    }

    fn sherpa_dir(&self) -> PathBuf {
        self.root.join("sherpa-onnx")
    }

    fn sherpa_exe(&self, program: &str) -> Option<PathBuf> {
        let name = exe_name(program);
        let local = sherpa_package()
            .map(|package| self.sherpa_dir().join(package.dir).join("bin").join(&name));
        find_program(local, &name)
    }

    /// The Piper executable: the downloaded one, or `piper` on `PATH`
    /// outside Windows.
    pub fn piper_exe(&self) -> Option<PathBuf> {
        let name = exe_name("piper");
        let local = piper_package().map(|package| self.piper_dir().join(package.dir).join(&name));
        find_program(local, &name)
    }

    pub fn is_installed(&self, id: ModelId) -> bool {
        match id {
            ModelId::Whisper(m) => {
                let dir = self.whisper_dir(m);
                WHISPER_FILES.iter().all(|f| dir.join(f).is_file())
            }
            ModelId::Voice(v) => {
                let (onnx, json) = self.voice_files(v);
                onnx.is_file() && json.is_file()
            }
            ModelId::PiperRuntime => self.piper_exe().is_some(),
            ModelId::KokoroRuntime => self.kokoro_exe().is_some(),
            ModelId::KokoroModel => kokoro_model_complete(&self.kokoro_dir()),
            ModelId::Parakeet => parakeet_complete(&self.parakeet_dir()),
        }
    }

    /// Bytes used on disk by an installed model.
    pub fn size_on_disk(&self, id: ModelId) -> u64 {
        let paths: Vec<PathBuf> = match id {
            ModelId::Whisper(m) => vec![self.whisper_dir(m)],
            ModelId::Voice(v) => {
                let (a, b) = self.voice_files(v);
                vec![a, b]
            }
            ModelId::PiperRuntime => vec![self.piper_dir()],
            ModelId::KokoroRuntime => vec![self.sherpa_dir()],
            ModelId::KokoroModel => vec![self.root.join("kokoro")],
            ModelId::Parakeet => vec![self.root.join("parakeet")],
        };
        paths.iter().map(|p| disk_usage(p)).sum()
    }

    pub fn remove(&self, id: ModelId) -> Result<(), AudioError> {
        let remove_dir = |dir: PathBuf| match fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(AudioError::io(dir, e)),
            _ => Ok(()),
        };
        match id {
            ModelId::Whisper(m) => remove_dir(self.whisper_dir(m)),
            ModelId::PiperRuntime => remove_dir(self.piper_dir()),
            ModelId::KokoroRuntime => remove_dir(self.sherpa_dir()),
            ModelId::KokoroModel => remove_dir(self.root.join("kokoro")),
            ModelId::Parakeet => remove_dir(self.root.join("parakeet")),
            ModelId::Voice(v) => {
                let (a, b) = self.voice_files(v);
                for path in [a, b] {
                    match fs::remove_file(&path) {
                        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                            return Err(AudioError::io(path, e));
                        }
                        _ => {}
                    }
                }
                Ok(())
            }
        }
    }

    /// Downloads and installs a model. `progress` gets `(bytes_done,
    /// bytes_expected)`. Setting `cancel` stops the download.
    pub fn download(
        &self,
        id: ModelId,
        progress: &dyn Fn(u64, u64),
        cancel: &AtomicBool,
    ) -> Result<(), AudioError> {
        let mut fetcher = Fetcher::new(id.approx_bytes(), progress, cancel);
        match id {
            ModelId::Whisper(m) => {
                let dir = self.whisper_dir(m);
                for file in WHISPER_FILES {
                    let url = format!("{HF}/openai/{}/resolve/main/{file}", m.id());
                    fetcher.fetch(&url, &dir.join(file))?;
                }
            }
            ModelId::Voice(v) => {
                let (onnx, json) = self.voice_files(v);
                let base = v.download_base();
                fetcher.fetch(&format!("{base}.onnx.json"), &json)?;
                fetcher.fetch(&format!("{base}.onnx"), &onnx)?;
            }
            ModelId::PiperRuntime => {
                let package = piper_package().ok_or_else(|| {
                    AudioError::Unsupported("no official piper build for this system".into())
                })?;
                let exe = Path::new(package.dir).join(exe_name("piper"));
                self.install_bundle(
                    &mut fetcher,
                    package.url,
                    Some(package.sha256),
                    &self.piper_dir(),
                    &|unpacked| unpacked.join(&exe).is_file(),
                )?;
            }
            ModelId::KokoroRuntime => {
                let package = sherpa_package().ok_or_else(|| {
                    AudioError::Unsupported("no official sherpa-onnx build for this system".into())
                })?;
                self.install_bundle(
                    &mut fetcher,
                    package.url,
                    Some(package.sha256),
                    &self.sherpa_dir(),
                    &|unpacked| sherpa_complete(&unpacked.join(package.dir)),
                )?;
            }
            ModelId::KokoroModel => self.install_bundle(
                &mut fetcher,
                KOKORO_MODEL_URL,
                None,
                &self.root.join("kokoro"),
                &|unpacked| kokoro_model_complete(&unpacked.join(KOKORO_MODEL_DIR)),
            )?,
            ModelId::Parakeet => self.install_bundle(
                &mut fetcher,
                PARAKEET_URL,
                None,
                &self.root.join("parakeet"),
                &|unpacked| parakeet_complete(&unpacked.join(PARAKEET_DIR)),
            )?,
        }
        tracing::info!(?id, "model installed");
        Ok(())
    }

    /// Downloads an archive, checks it, and moves its contents to `dest`.
    /// The bundle is staged first, so an interruption never leaves an
    /// installed-looking tree.
    fn install_bundle(
        &self,
        fetcher: &mut Fetcher<'_>,
        url: &str,
        sha256: Option<&str>,
        dest: &Path,
        complete: &dyn Fn(&Path) -> bool,
    ) -> Result<(), AudioError> {
        fs::create_dir_all(&self.root).map_err(|e| AudioError::io(&self.root, e))?;
        let staging =
            tempfile::tempdir_in(&self.root).map_err(|e| AudioError::io(&self.root, e))?;
        let archive = staging.path().join("download");
        fetcher.fetch(url, &archive)?;
        if let Some(expected) = sha256 {
            check_sha256(&archive, expected, url)?;
        }
        let unpacked = staging.path().join("unpacked");
        unpack(&archive, ArchiveKind::of(url), &unpacked, fetcher.cancel)?;
        if !complete(&unpacked) {
            return Err(AudioError::Model(format!("incomplete bundle from {url}")));
        }
        if fetcher.cancel.load(Ordering::Relaxed) {
            return Err(AudioError::Cancelled);
        }
        if dest.exists() {
            fs::remove_dir_all(dest).map_err(|e| AudioError::io(dest, e))?;
        }
        fs::rename(&unpacked, dest).map_err(|e| AudioError::io(dest, e))
    }
}

const WHISPER_FILES: [&str; 3] = ["config.json", "tokenizer.json", "model.safetensors"];

fn kokoro_model_complete(dir: &Path) -> bool {
    [
        "model.onnx",
        "voices.bin",
        "tokens.txt",
        "espeak-ng-data/phontab",
        "espeak-ng-data/phonindex",
        "espeak-ng-data/phondata",
        "espeak-ng-data/intonations",
    ]
    .iter()
    .all(|name| fs::metadata(dir.join(name)).is_ok_and(|m| m.is_file() && m.len() > 0))
}

fn parakeet_complete(dir: &Path) -> bool {
    PARAKEET_FILES
        .iter()
        .all(|name| fs::metadata(dir.join(name)).is_ok_and(|m| m.is_file() && m.len() > 0))
}

/// The sherpa-onnx programs and the C library the app loads.
fn sherpa_complete(dir: &Path) -> bool {
    let bin = dir.join("bin");
    let library = if cfg!(windows) {
        "sherpa-onnx-c-api.dll"
    } else if cfg!(target_os = "macos") {
        "libsherpa-onnx-c-api.dylib"
    } else {
        "libsherpa-onnx-c-api.so"
    };
    bin.join(exe_name("sherpa-onnx-offline-tts")).is_file()
        && bin
            .join(exe_name("sherpa-onnx-offline-websocket-server"))
            .is_file()
        && dir.join("lib").join(library).is_file()
}

fn check_sha256(path: &Path, expected: &str, url: &str) -> Result<(), AudioError> {
    use sha2::{Digest, Sha256};
    let mut file = fs::File::open(path).map_err(|e| AudioError::io(path, e))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| AudioError::io(path, e))?;
    let actual = format!("{:x}", hasher.finalize());
    if actual == expected {
        Ok(())
    } else {
        Err(AudioError::Download {
            url: url.to_owned(),
            reason: format!("checksum mismatch: expected {expected}, got {actual}"),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    TarGz,
    TarBz2,
}

impl ArchiveKind {
    fn of(url: &str) -> Self {
        if url.ends_with(".zip") {
            Self::Zip
        } else if url.ends_with(".tar.gz") || url.ends_with(".tgz") {
            Self::TarGz
        } else {
            Self::TarBz2
        }
    }
}

fn unpack(
    path: &Path,
    kind: ArchiveKind,
    dest: &Path,
    cancel: &AtomicBool,
) -> Result<(), AudioError> {
    fs::create_dir_all(dest).map_err(|e| AudioError::io(dest, e))?;
    let file = fs::File::open(path).map_err(|e| AudioError::io(path, e))?;
    match kind {
        ArchiveKind::Zip => zip::ZipArchive::new(file)
            .and_then(|mut zip| zip.extract(dest))
            .map_err(|e| AudioError::Model(format!("could not extract {}: {e}", path.display()))),
        ArchiveKind::TarGz => unpack_tar(flate2::read::GzDecoder::new(file), path, dest, cancel),
        ArchiveKind::TarBz2 => unpack_tar(bzip2::read::BzDecoder::new(file), path, dest, cancel),
    }
}

fn unpack_tar(
    reader: impl Read,
    path: &Path,
    dest: &Path,
    cancel: &AtomicBool,
) -> Result<(), AudioError> {
    let mut archive = tar::Archive::new(reader);
    let entries = archive.entries().map_err(|e| AudioError::io(path, e))?;
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            return Err(AudioError::Cancelled);
        }
        let mut entry = entry.map_err(|e| AudioError::io(path, e))?;
        let kind = entry.header().entry_type();
        if kind.is_symlink() {
            // Linux and macOS builds link `libx.so` to `libx.so.1.2` in the
            // same folder; nothing may point anywhere else.
            let target = entry.link_name().map_err(|e| AudioError::io(path, e))?;
            if !target.as_deref().is_some_and(is_sibling_name) {
                return Err(AudioError::Model("unsafe link in model archive".into()));
            }
        } else if !kind.is_file() && !kind.is_dir() {
            return Err(AudioError::Model(
                "unsupported entry in model archive".into(),
            ));
        }
        if !entry.unpack_in(dest).map_err(|e| AudioError::io(dest, e))? {
            return Err(AudioError::Model("unsafe path in model archive".into()));
        }
    }
    Ok(())
}

/// A plain file name: no folders, no `..`, not absolute.
fn is_sibling_name(target: &Path) -> bool {
    let mut parts = target.components();
    matches!(
        (parts.next(), parts.next()),
        (Some(std::path::Component::Normal(_)), None)
    )
}

struct Fetcher<'a> {
    agent: ureq::Agent,
    done: u64,
    expected: u64,
    progress: &'a dyn Fn(u64, u64),
    cancel: &'a AtomicBool,
}

impl<'a> Fetcher<'a> {
    fn new(expected: u64, progress: &'a dyn Fn(u64, u64), cancel: &'a AtomicBool) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_body(Some(Duration::from_secs(900)))
            .build()
            .into();
        Self {
            agent,
            done: 0,
            expected,
            progress,
            cancel,
        }
    }

    /// Streams `url` into `dest` through a `.part` file, so an interrupted
    /// download never looks installed.
    fn fetch(&mut self, url: &str, dest: &Path) -> Result<(), AudioError> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(AudioError::Cancelled);
        }
        let fail = |reason: String| AudioError::Download {
            url: url.to_owned(),
            reason,
        };
        if let Some(dir) = dest.parent() {
            fs::create_dir_all(dir).map_err(|e| AudioError::io(dir, e))?;
        }
        let part = dest.with_extension("part");
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| fail(e.to_string()))?;
        let mut reader = response.into_body().into_reader();
        let mut file = fs::File::create(&part).map_err(|e| AudioError::io(&part, e))?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            if self.cancel.load(Ordering::Relaxed) {
                drop(file);
                let _ = fs::remove_file(&part);
                return Err(AudioError::Cancelled);
            }
            let n = reader.read(&mut buf).map_err(|e| fail(e.to_string()))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])
                .map_err(|e| AudioError::io(&part, e))?;
            self.done += n as u64;
            self.expected = self.expected.max(self.done);
            (self.progress)(self.done, self.expected);
        }
        file.flush().map_err(|e| AudioError::io(&part, e))?;
        drop(file);
        fs::rename(&part, dest).map_err(|e| AudioError::io(dest, e))?;
        Ok(())
    }
}

fn disk_usage(path: &Path) -> u64 {
    match fs::metadata(path) {
        Ok(meta) if meta.is_file() => meta.len(),
        Ok(meta) if meta.is_dir() => fs::read_dir(path)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|e| disk_usage(&e.path()))
                    .sum()
            })
            .unwrap_or(0),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_bundle_into_a_new_staging_directory() {
        let root = tempfile::tempdir().unwrap();
        let archive_path = root.path().join("bundle.tar.bz2");
        let file = fs::File::create(&archive_path).unwrap();
        let encoder = bzip2::write::BzEncoder::new(file, bzip2::Compression::fast());
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(5);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, "bundle/model.onnx", b"model".as_slice())
            .unwrap();
        archive.into_inner().unwrap().finish().unwrap();
        let dest = root.path().join("not-yet-created");
        let kind = ArchiveKind::of("bundle.tar.bz2");
        unpack(&archive_path, kind, &dest, &AtomicBool::new(false)).unwrap();
        assert_eq!(fs::read(dest.join("bundle/model.onnx")).unwrap(), b"model");
        assert!(matches!(
            unpack(
                &archive_path,
                kind,
                &root.path().join("cancelled"),
                &AtomicBool::new(true)
            ),
            Err(AudioError::Cancelled)
        ));
    }

    /// A gzip tar with `piper/libx.so -> <target>` and the file it names.
    fn piper_like_archive(dir: &Path, target: &str) -> PathBuf {
        let path = dir.join("piper.tar.gz");
        let file = fs::File::create(&path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(3);
        header.set_mode(0o755);
        header.set_cksum();
        archive
            .append_data(&mut header, "piper/libx.so.1.2", b"lib".as_slice())
            .unwrap();
        let mut link = tar::Header::new_gnu();
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_size(0);
        link.set_mode(0o777);
        archive
            .append_link(&mut link, "piper/libx.so", target)
            .unwrap();
        archive.into_inner().unwrap().finish().unwrap();
        path
    }

    #[test]
    fn links_must_stay_in_their_folder() {
        let root = tempfile::tempdir().unwrap();
        for target in ["../../etc/passwd", "/etc/passwd", "sub/libx.so.1.2"] {
            let archive = piper_like_archive(root.path(), target);
            let result = unpack(
                &archive,
                ArchiveKind::TarGz,
                &root.path().join("out"),
                &AtomicBool::new(false),
            );
            assert!(matches!(result, Err(AudioError::Model(_))), "{target}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn library_links_next_to_their_file_are_kept() {
        let root = tempfile::tempdir().unwrap();
        let archive = piper_like_archive(root.path(), "libx.so.1.2");
        let dest = root.path().join("out");
        unpack(&archive, ArchiveKind::TarGz, &dest, &AtomicBool::new(false)).unwrap();
        assert_eq!(fs::read(dest.join("piper/libx.so")).unwrap(), b"lib");
        assert!(
            fs::symlink_metadata(dest.join("piper/libx.so"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn checksum_mismatch_is_a_download_error() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("file");
        fs::write(&path, b"abc").unwrap();
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        check_sha256(&path, abc, "u").unwrap();
        assert!(matches!(
            check_sha256(&path, &"0".repeat(64), "u"),
            Err(AudioError::Download { .. })
        ));
    }

    #[test]
    fn packages_are_pinned_and_match_their_archive() {
        for package in [piper_package(), sherpa_package()].into_iter().flatten() {
            assert_eq!(package.sha256.len(), 64, "{}", package.url);
            assert!(package.url.starts_with("https://github.com/"));
            assert!(package.url.contains(package.dir) || package.dir == "piper");
        }
        // Every desktop the app ships for can download its speech runtime.
        if cfg!(any(
            all(windows, target_arch = "x86_64"),
            all(
                target_os = "linux",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ),
            target_os = "macos"
        )) {
            assert!(ModelId::KokoroRuntime.downloadable());
        }
        if cfg!(any(windows, target_os = "linux")) && !cfg!(target_arch = "arm") {
            assert!(ModelId::PiperRuntime.downloadable());
        }
        assert_eq!(
            ArchiveKind::of("a/piper_windows_amd64.zip"),
            ArchiveKind::Zip
        );
        assert_eq!(
            ArchiveKind::of("a/piper_linux_x86_64.tar.gz"),
            ArchiveKind::TarGz
        );
        assert_eq!(ArchiveKind::of("a/x.tar.bz2"), ArchiveKind::TarBz2);
    }

    #[test]
    fn incomplete_kokoro_model_is_not_installed() {
        let root = tempfile::tempdir().unwrap();
        let store = ModelStore::new(root.path());
        fs::create_dir_all(store.kokoro_dir()).unwrap();
        fs::write(store.kokoro_dir().join("model.onnx"), b"model").unwrap();
        assert!(!store.is_installed(ModelId::KokoroModel));
        store.remove(ModelId::KokoroModel).unwrap();
        assert!(!store.kokoro_dir().exists());
    }

    #[test]
    fn nothing_installed_in_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path());
        for id in ModelId::all() {
            if matches!(id, ModelId::PiperRuntime | ModelId::KokoroRuntime) && !cfg!(windows) {
                continue;
            }
            assert!(!store.is_installed(id), "{id:?}");
        }
    }

    #[test]
    fn voice_counts_as_installed_with_both_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path());
        let (onnx, json) = store.voice_files(PiperVoice::Faber);
        fs::create_dir_all(onnx.parent().unwrap()).unwrap();
        fs::write(&onnx, b"x").unwrap();
        assert!(!store.is_installed(ModelId::Voice(PiperVoice::Faber)));
        fs::write(&json, b"{}").unwrap();
        assert!(store.is_installed(ModelId::Voice(PiperVoice::Faber)));
        assert_eq!(store.size_on_disk(ModelId::Voice(PiperVoice::Faber)), 3);
        store.remove(ModelId::Voice(PiperVoice::Faber)).unwrap();
        assert!(!store.is_installed(ModelId::Voice(PiperVoice::Faber)));
    }

    #[test]
    fn parakeet_needs_all_its_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path());
        let model = store.parakeet_dir();
        fs::create_dir_all(&model).unwrap();
        for name in &PARAKEET_FILES[..3] {
            fs::write(model.join(name), b"x").unwrap();
        }
        assert!(!store.is_installed(ModelId::Parakeet));
        fs::write(model.join("tokens.txt"), b"x").unwrap();
        assert!(store.is_installed(ModelId::Parakeet));
        store.remove(ModelId::Parakeet).unwrap();
        assert!(!store.is_installed(ModelId::Parakeet));
    }

    #[test]
    fn removing_missing_model_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path());
        store.remove(ModelId::Whisper(WhisperModel::Tiny)).unwrap();
    }
}
