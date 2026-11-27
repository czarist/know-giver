//! Moving a student's study to another computer through one file: study
//! plans, progress, handouts, photos of written answers and preferences.
//!
//! What belongs to one computer stays out of the file: the AI service, its
//! key and models, account sessions, the cloud voice service, the
//! handwriting-reading choice and the downloaded speech models. On the new
//! computer the student sets the AI up again; everything else is there.
//!
//! The file is a zip (`.kgbackup`) with a manifest, a consistent copy of the
//! database and the study files. Importing replaces this computer's study,
//! after keeping a backup of it in `backups/`.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use kg_study::{SCHEMA_VERSION, Store};
use serde::{Deserialize, Serialize};

use crate::{AppPaths, Engine, EngineError, Event, Notice, Settings};

/// File extension of a backup.
pub const BACKUP_EXTENSION: &str = "kgbackup";
/// Version of the backup layout; a newer one is refused.
const FORMAT: u32 = 1;
const MANIFEST: &str = "manifest.json";
const DATABASE: &str = "know-giver.db";
/// Study folders under the data folder that travel with the database.
const STUDY_DIRS: [&str; 2] = ["handouts", "discursive"];
/// Where an import waits until the app reopens its data.
const STAGING: &str = "import-staging";

/// Why a file could not be imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupProblem {
    /// Not a backup made by this app, or damaged.
    NotABackup,
    /// Made by a newer version of the app.
    TooNew,
}

impl std::fmt::Display for BackupProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotABackup => "not a study backup, or damaged",
            Self::TooNew => "backup made by a newer version",
        })
    }
}

/// Written at the root of every backup.
#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    format: u32,
    app_version: String,
    schema_version: u32,
    created_at: String,
    /// The data folder on the computer that made the backup, so file paths
    /// saved in the database can follow the new one.
    data_dir: String,
}

#[derive(Debug)]
pub(crate) enum BackupEvent {
    Exported(Result<PathBuf, EngineError>),
    /// The backup was unpacked next to the current data, ready to replace it.
    Staged(Result<(), EngineError>),
}

/// An export or import running in the background.
#[derive(Debug, Default)]
pub(crate) struct BackupState {
    busy: bool,
    /// The app should reopen its data from the staged import.
    reload: bool,
}

impl Settings {
    /// The settings without what belongs to this computer, for a backup.
    pub(crate) fn without_machine_data(&self) -> Settings {
        let mut out = self.clone();
        out.ai = kg_ai::AiConfig::default();
        out.ai_provider.clear();
        out.cloud_speech = kg_audio::CloudSpeechConfig::default();
        out.tts_backend = kg_audio::TtsBackend::default();
        out.manuscript_ocr = crate::ManuscriptOcr::default();
        out.stt_engine_chosen = false;
        out.ai_setup_pending = false;
        out
    }

    /// Imported settings, keeping what belongs to this computer from `here`.
    /// When this computer has no AI service yet, the first-run setup opens
    /// on the AI step.
    pub(crate) fn with_machine_data_from(mut self, here: &Settings) -> Settings {
        self.ai = here.ai.clone();
        self.ai_provider.clone_from(&here.ai_provider);
        self.cloud_speech = here.cloud_speech.clone();
        self.tts_backend = here.tts_backend;
        self.manuscript_ocr = here.manuscript_ocr;
        self.stt_engine = here.stt_engine;
        self.stt_engine_chosen = here.stt_engine_chosen;
        self.whisper_model = here.whisper_model;
        self.ai_setup_pending = !here.ai_ready();
        self
    }

    /// An AI service was set up on this computer: picked in the setup, or
    /// changed from the untouched default (which points at a local Ollama).
    pub(crate) fn ai_ready(&self) -> bool {
        !self.ai_provider.trim().is_empty() || self.ai != kg_ai::AiConfig::default()
    }
}

impl Engine {
    /// An export or import is running.
    pub fn backup_busy(&self) -> bool {
        self.backup.busy
    }

    /// True once, when an import is ready: the app then closes this engine,
    /// calls [`finish_import`] and opens a new one.
    pub fn take_reload_request(&mut self) -> bool {
        std::mem::take(&mut self.backup.reload)
    }

    /// The `.env` AI settings this engine was started with, to start the next
    /// one the same way.
    pub fn env_ai(&self) -> &kg_ai::AiConfig {
        &self.env_ai
    }

    /// Writes the student's study to `path` (see the module notes).
    pub(crate) fn export_backup(&mut self, path: PathBuf) -> Result<(), EngineError> {
        if self.backup.busy {
            return Ok(());
        }
        let work = work_dir(&self.paths)?;
        let db = work.path().join(DATABASE);
        self.store.copy_to(&db)?;
        self.backup.busy = true;
        let data_dir = self.paths.data_dir.clone();
        self.spawn(move || {
            let result = write_backup(&db, &data_dir, &path).map(|()| path);
            drop(work);
            Event::Backup(BackupEvent::Exported(result))
        });
        Ok(())
    }

    /// Unpacks the backup at `path` to replace this computer's study, after
    /// saving a backup of the current one in `backups/`.
    pub(crate) fn import_backup(&mut self, path: PathBuf) -> Result<(), EngineError> {
        if self.backup.busy {
            return Ok(());
        }
        let staging = self.paths.data_dir.join(STAGING);
        remove_dir(&staging);
        create_dir(&staging)?;
        let current = staging.join("current.db");
        self.store.copy_to(&current)?;
        self.backup.busy = true;
        let data_dir = self.paths.data_dir.clone();
        self.spawn(move || {
            let result = keep_current(&current, &data_dir)
                .and_then(|()| stage(&path, &data_dir, &staging.join("new")));
            Event::Backup(BackupEvent::Staged(result))
        });
        Ok(())
    }

    pub(crate) fn on_backup_event(&mut self, event: BackupEvent) -> Result<(), EngineError> {
        self.backup.busy = false;
        match event {
            BackupEvent::Exported(result) => self.notices.push(Notice::Exported(result?)),
            BackupEvent::Staged(result) => {
                if let Err(err) = result.and_then(|()| self.merge_staged_settings()) {
                    remove_dir(&self.paths.data_dir.join(STAGING));
                    return Err(err);
                }
                self.backup.reload = true;
            }
        }
        Ok(())
    }

    /// Gives the staged database this computer's AI and account sessions.
    fn merge_staged_settings(&mut self) -> Result<(), EngineError> {
        let mut staged = Store::open(&staged_dir(&self.paths).join(DATABASE))?;
        let imported = Settings::load(&staged, &kg_ai::AiConfig::default())?;
        let merged = imported.with_machine_data_from(&self.settings);
        merged.save(&mut staged, &self.env_ai)?;
        for key in crate::accounts::SESSION_KEYS {
            match self.store.setting(key)? {
                Some(value) => staged.set_setting(key, &value)?,
                None => staged.remove_setting(key)?,
            }
        }
        Ok(())
    }
}

/// Puts a staged import in place of the current data. Call with no engine
/// open on `paths` (its database file must be closed).
pub fn finish_import(paths: &AppPaths) -> Result<(), EngineError> {
    let staged = staged_dir(paths);
    let staged_db = staged.join(DATABASE);
    if !staged_db.exists() {
        return Ok(());
    }
    let db = paths.database();
    let aside = db.with_extension("db.before-import");
    let _ = fs::remove_file(&aside);
    for suffix in ["-wal", "-shm"] {
        let _ = fs::remove_file(PathBuf::from(format!("{}{suffix}", db.display())));
    }
    if db.exists() {
        rename(&db, &aside)?;
    }
    if let Err(err) = rename(&staged_db, &db) {
        let _ = fs::rename(&aside, &db);
        return Err(err);
    }
    for name in STUDY_DIRS {
        let live = paths.data_dir.join(name);
        remove_dir(&live);
        let incoming = staged.join(name);
        if incoming.exists() {
            rename(&incoming, &live)?;
        }
    }
    let _ = fs::remove_file(&aside);
    remove_dir(&paths.data_dir.join(STAGING));
    Ok(())
}

/// Drops an import left unfinished (the app closed before it reopened).
pub(crate) fn discard_staging(paths: &AppPaths) {
    remove_dir(&paths.data_dir.join(STAGING));
}

/// A name for a new backup, e.g. `KnowGiver-backup-2026-11-26.kgbackup`.
pub fn backup_file_name(today: chrono::NaiveDate) -> String {
    format!("KnowGiver-backup-{today}.{BACKUP_EXTENSION}")
}

fn staged_dir(paths: &AppPaths) -> PathBuf {
    paths.data_dir.join(STAGING).join("new")
}

fn work_dir(paths: &AppPaths) -> Result<tempfile::TempDir, EngineError> {
    tempfile::Builder::new()
        .prefix("backup-")
        .tempdir_in(&paths.data_dir)
        .map_err(|source| EngineError::Export {
            path: paths.data_dir.clone(),
            source,
        })
}

/// Saves the study as it is now in `backups/`, before an import replaces it.
fn keep_current(current_db: &Path, data_dir: &Path) -> Result<(), EngineError> {
    let folder = data_dir.join("backups");
    create_dir(&folder)?;
    let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S");
    let path = folder.join(format!("before-import-{stamp}.{BACKUP_EXTENSION}"));
    let result = write_backup(current_db, data_dir, &path);
    let _ = fs::remove_file(current_db);
    result
}

/// Removes this computer's data from the database copy at `db` and packs it
/// with the study folders of `data_dir` into `path`.
fn write_backup(db: &Path, data_dir: &Path, path: &Path) -> Result<(), EngineError> {
    {
        let mut copy = Store::open(db)?;
        let settings = Settings::load(&copy, &kg_ai::AiConfig::default())?;
        settings
            .without_machine_data()
            .save(&mut copy, &kg_ai::AiConfig::default())?;
        for key in crate::accounts::SESSION_KEYS {
            copy.remove_setting(key)?;
        }
    }
    let manifest = Manifest {
        format: FORMAT,
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        schema_version: SCHEMA_VERSION,
        created_at: chrono::Local::now().to_rfc3339(),
        data_dir: data_dir.display().to_string(),
    };
    let part = path.with_extension(format!("{BACKUP_EXTENSION}.part"));
    let write_error = |source: std::io::Error| EngineError::Export {
        path: path.to_path_buf(),
        source,
    };
    let zip_error = |e: zip::result::ZipError| write_error(std::io::Error::other(e));
    let file = fs::File::create(&part).map_err(write_error)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| write_error(e.into()))?;
    zip.start_file(MANIFEST, options).map_err(zip_error)?;
    zip.write_all(&json).map_err(write_error)?;
    add_file(&mut zip, db, DATABASE, options).map_err(write_error)?;
    for name in STUDY_DIRS {
        add_dir(&mut zip, &data_dir.join(name), name, options).map_err(write_error)?;
    }
    zip.finish().map_err(zip_error)?;
    let finished = fs::rename(&part, path).map_err(write_error);
    if finished.is_err() {
        let _ = fs::remove_file(&part);
    }
    finished
}

fn add_file(
    zip: &mut zip::ZipWriter<fs::File>,
    source: &Path,
    name: &str,
    options: zip::write::SimpleFileOptions,
) -> std::io::Result<()> {
    zip.start_file(name, options)
        .map_err(std::io::Error::other)?;
    let mut file = fs::File::open(source)?;
    std::io::copy(&mut file, zip)?;
    Ok(())
}

/// Adds every file under `dir` as `prefix/...`; a missing folder adds nothing.
fn add_dir(
    zip: &mut zip::ZipWriter<fs::File>,
    dir: &Path,
    prefix: &str,
    options: zip::write::SimpleFileOptions,
) -> std::io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    for entry in entries {
        let entry = entry?;
        let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            add_dir(zip, &entry.path(), &name, options)?;
        } else if kind.is_file() {
            add_file(zip, &entry.path(), &name, options)?;
        }
    }
    Ok(())
}

/// Checks and unpacks the backup at `path` into `dest`, and points the
/// database's file paths at this computer's data folder.
fn stage(path: &Path, data_dir: &Path, dest: &Path) -> Result<(), EngineError> {
    let not_a_backup = || EngineError::Backup(BackupProblem::NotABackup);
    let file = fs::File::open(path).map_err(|_| not_a_backup())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| not_a_backup())?;
    let manifest: Manifest = {
        let mut entry = archive.by_name(MANIFEST).map_err(|_| not_a_backup())?;
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .map_err(|_| not_a_backup())?;
        serde_json::from_str(&text).map_err(|_| not_a_backup())?
    };
    if manifest.format > FORMAT || manifest.schema_version > SCHEMA_VERSION {
        return Err(EngineError::Backup(BackupProblem::TooNew));
    }
    create_dir(dest)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|_| not_a_backup())?;
        // Only names inside the folder; never `..` or an absolute path.
        let Some(name) = entry.enclosed_name() else {
            return Err(not_a_backup());
        };
        let top = name
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned());
        let wanted = name == Path::new(DATABASE)
            || top.is_some_and(|top| STUDY_DIRS.contains(&top.as_str()));
        if !wanted || entry.is_dir() {
            continue;
        }
        let out_path = dest.join(&name);
        if let Some(parent) = out_path.parent() {
            create_dir(parent)?;
        }
        let mut out = fs::File::create(&out_path).map_err(|source| EngineError::Export {
            path: out_path.clone(),
            source,
        })?;
        std::io::copy(&mut entry, &mut out).map_err(|_| not_a_backup())?;
    }
    let db = dest.join(DATABASE);
    if !db.exists() {
        return Err(not_a_backup());
    }
    let mut store = Store::open(&db).map_err(|_| not_a_backup())?;
    let old_root = manifest.data_dir;
    store.rewrite_file_paths(|saved| moved_path(saved, &old_root, data_dir))?;
    Ok(())
}

/// `saved` moved from the data folder `old_root` to `new_root`, when it was
/// inside it. Works across systems (`\` and `/`).
fn moved_path(saved: &str, old_root: &str, new_root: &Path) -> Option<String> {
    let normalize = |s: &str| s.replace('\\', "/");
    let saved_n = normalize(saved);
    let root_n = normalize(old_root);
    let root_n = root_n.trim_end_matches('/');
    let rest = saved_n.strip_prefix(root_n)?.strip_prefix('/')?;
    let mut path = new_root.to_path_buf();
    for part in rest.split('/').filter(|p| !p.is_empty()) {
        path.push(part);
    }
    Some(path.display().to_string())
}

fn create_dir(path: &Path) -> Result<(), EngineError> {
    fs::create_dir_all(path).map_err(|source| EngineError::Export {
        path: path.to_path_buf(),
        source,
    })
}

fn rename(from: &Path, to: &Path) -> Result<(), EngineError> {
    fs::rename(from, to).map_err(|source| EngineError::Export {
        path: to.to_path_buf(),
        source,
    })
}

fn remove_dir(path: &Path) {
    if let Err(err) = fs::remove_dir_all(path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%err, path = %path.display(), "could not remove folder");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_follow_the_new_data_folder_across_systems() {
        let new_root = Path::new("/home/ana/.local/share/KnowGiver");
        assert_eq!(
            moved_path(
                r"C:\Users\ana\AppData\Local\KnowGiver\handouts\crase.pdf",
                r"C:\Users\ana\AppData\Local\KnowGiver",
                new_root,
            ),
            Some(
                new_root
                    .join("handouts")
                    .join("crase.pdf")
                    .display()
                    .to_string()
            )
        );
        // Files outside the old data folder are left as they are.
        assert_eq!(
            moved_path(
                r"D:\Docs\x.pdf",
                r"C:\Users\ana\AppData\Local\KnowGiver",
                new_root
            ),
            None
        );
        // A folder whose name only starts the same is not inside it.
        assert_eq!(
            moved_path(r"C:\KnowGiverOld\a.pdf", r"C:\KnowGiver", new_root),
            None
        );
    }

    #[test]
    fn machine_data_stays_out_and_is_kept_on_import() {
        let mut there = Settings {
            student_name: "Ana".into(),
            quiz_size: 8,
            ai_provider: "zai".into(),
            ..Settings::default()
        };
        there.ai.base_url = "https://api.example/v1".into();
        there.ai.api_key = "secret".into();
        there.ai.models.tutor = "model".into();
        there.cloud_speech.api_key = "voice-secret".into();

        let exported = there.without_machine_data();
        assert_eq!(exported.ai, kg_ai::AiConfig::default());
        assert!(exported.ai_provider.is_empty());
        assert!(exported.cloud_speech.api_key.is_empty());
        assert_eq!(exported.student_name, "Ana");
        assert_eq!(exported.quiz_size, 8);

        // A new computer without AI: the setup opens on the AI step.
        let fresh = exported
            .clone()
            .with_machine_data_from(&Settings::default());
        assert!(fresh.ai_setup_pending);
        assert!(fresh.needs_setup());
        assert_eq!(fresh.quiz_size, 8);

        // A computer with its own AI keeps it.
        let imported = exported.with_machine_data_from(&there);
        assert!(!imported.ai_setup_pending);
        assert_eq!(imported.ai.api_key, "secret");
        assert_eq!(imported.ai_provider, "zai");
    }
}
