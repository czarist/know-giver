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
const PIPER_RELEASE: &str =
    "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip";
const KOKORO_RUNTIME_DIR: &str = "sherpa-onnx-v1.12.20-win-x64-shared";
const KOKORO_RUNTIME_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.12.20/sherpa-onnx-v1.12.20-win-x64-shared.tar.bz2";
const KOKORO_MODEL_DIR: &str = "kokoro-multi-lang-v1_0";
const KOKORO_MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-multi-lang-v1_0.tar.bz2";

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

/// Brazilian Portuguese Piper voices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PiperVoice {
    #[default]
    Faber,
    Cadu,
    Jeff,
}

impl PiperVoice {
    pub const ALL: [Self; 3] = [Self::Faber, Self::Cadu, Self::Jeff];

    pub fn id(self) -> &'static str {
        match self {
            Self::Faber => "pt_BR-faber-medium",
            Self::Cadu => "pt_BR-cadu-medium",
            Self::Jeff => "pt_BR-jeff-medium",
        }
    }

    fn speaker(self) -> &'static str {
        match self {
            Self::Faber => "faber",
            Self::Cadu => "cadu",
            Self::Jeff => "jeff",
        }
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
}

impl ModelId {
    pub fn all() -> Vec<Self> {
        let mut all: Vec<Self> = WhisperModel::ALL.into_iter().map(Self::Whisper).collect();
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
            Self::PiperRuntime => 22_000_000,
            Self::KokoroRuntime => 26_700_000,
            Self::KokoroModel => 350_000_000,
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

    /// Downloaded runtime on Windows, or an explicitly installed executable on PATH.
    pub fn kokoro_exe(&self) -> Option<PathBuf> {
        let name = if cfg!(windows) {
            "sherpa-onnx-offline-tts.exe"
        } else {
            "sherpa-onnx-offline-tts"
        };
        let local = self
            .root
            .join("sherpa-onnx")
            .join(KOKORO_RUNTIME_DIR)
            .join("bin")
            .join(name);
        if local.is_file() {
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

    /// The Piper executable: the downloaded one, or `piper` on `PATH`
    /// outside Windows.
    pub fn piper_exe(&self) -> Option<PathBuf> {
        let name = if cfg!(windows) { "piper.exe" } else { "piper" };
        let local = self.piper_dir().join("piper").join(name);
        if local.is_file() {
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
            ModelId::KokoroRuntime => vec![self.root.join("sherpa-onnx")],
            ModelId::KokoroModel => vec![self.root.join("kokoro")],
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
            ModelId::KokoroRuntime => remove_dir(self.root.join("sherpa-onnx")),
            ModelId::KokoroModel => remove_dir(self.root.join("kokoro")),
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
                let base = format!(
                    "{HF}/rhasspy/piper-voices/resolve/main/pt/pt_BR/{}/medium/{}",
                    v.speaker(),
                    v.id()
                );
                fetcher.fetch(&format!("{base}.onnx.json"), &json)?;
                fetcher.fetch(&format!("{base}.onnx"), &onnx)?;
            }
            ModelId::PiperRuntime => {
                if !cfg!(windows) {
                    return Err(AudioError::Unsupported(
                        "install piper from your package manager".into(),
                    ));
                }
                let zip_path = self.root.join("piper.zip");
                fetcher.fetch(PIPER_RELEASE, &zip_path)?;
                let result = unzip(&zip_path, &self.piper_dir());
                let _ = fs::remove_file(&zip_path);
                result?;
            }
            ModelId::KokoroRuntime | ModelId::KokoroModel => {
                if id == ModelId::KokoroRuntime && !cfg!(all(windows, target_arch = "x86_64")) {
                    return Err(AudioError::Unsupported(
                        "install sherpa-onnx v1.12.20 on PATH".into(),
                    ));
                }
                let (url, directory) = if id == ModelId::KokoroRuntime {
                    (KOKORO_RUNTIME_URL, "sherpa-onnx")
                } else {
                    (KOKORO_MODEL_URL, "kokoro")
                };
                fs::create_dir_all(&self.root).map_err(|e| AudioError::io(&self.root, e))?;
                // Stage the whole bundle: interruption never leaves an installed-looking tree.
                let staging =
                    tempfile::tempdir_in(&self.root).map_err(|e| AudioError::io(&self.root, e))?;
                let archive = staging.path().join("download.tar.bz2");
                fetcher.fetch(url, &archive)?;
                let unpacked = staging.path().join("unpacked");
                unpack_bzip_tar(&archive, &unpacked, cancel)?;
                let complete = match id {
                    ModelId::KokoroRuntime => unpacked
                        .join(KOKORO_RUNTIME_DIR)
                        .join("bin/sherpa-onnx-offline-tts.exe")
                        .is_file(),
                    _ => kokoro_model_complete(&unpacked.join(KOKORO_MODEL_DIR)),
                };
                if !complete {
                    return Err(AudioError::Tts("incomplete Kokoro bundle".into()));
                }
                if cancel.load(Ordering::Relaxed) {
                    return Err(AudioError::Cancelled);
                }
                let dest = self.root.join(directory);
                if dest.exists() {
                    fs::remove_dir_all(&dest).map_err(|e| AudioError::io(&dest, e))?;
                }
                fs::rename(&unpacked, &dest).map_err(|e| AudioError::io(&dest, e))?;
            }
        }
        tracing::info!(?id, "model installed");
        Ok(())
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

fn unpack_bzip_tar(path: &Path, dest: &Path, cancel: &AtomicBool) -> Result<(), AudioError> {
    fs::create_dir_all(dest).map_err(|e| AudioError::io(dest, e))?;
    let file = fs::File::open(path).map_err(|e| AudioError::io(path, e))?;
    let mut archive = tar::Archive::new(bzip2::read::BzDecoder::new(file));
    let entries = archive.entries().map_err(|e| AudioError::io(path, e))?;
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            return Err(AudioError::Cancelled);
        }
        let mut entry = entry.map_err(|e| AudioError::io(path, e))?;
        // Model/runtime bundles contain regular files and directories only.
        if !entry.header().entry_type().is_file() && !entry.header().entry_type().is_dir() {
            return Err(AudioError::Tts("unsupported link in model archive".into()));
        }
        if !entry.unpack_in(dest).map_err(|e| AudioError::io(dest, e))? {
            return Err(AudioError::Tts("unsafe path in model archive".into()));
        }
    }
    Ok(())
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

fn unzip(archive: &Path, dest: &Path) -> Result<(), AudioError> {
    let file = fs::File::open(archive).map_err(|e| AudioError::io(archive, e))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| AudioError::Tts(format!("bad piper archive: {e}")))?;
    zip.extract(dest)
        .map_err(|e| AudioError::Tts(format!("could not extract piper: {e}")))
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
        unpack_bzip_tar(&archive_path, &dest, &AtomicBool::new(false)).unwrap();
        assert_eq!(fs::read(dest.join("bundle/model.onnx")).unwrap(), b"model");
        assert!(matches!(
            unpack_bzip_tar(
                &archive_path,
                &root.path().join("cancelled"),
                &AtomicBool::new(true)
            ),
            Err(AudioError::Cancelled)
        ));
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
    fn removing_missing_model_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path());
        store.remove(ModelId::Whisper(WhisperModel::Tiny)).unwrap();
    }
}
