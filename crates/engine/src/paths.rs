//! Where the app keeps its data.

use std::path::{Path, PathBuf};

/// Data locations. Defaults to the OS local data folder
/// (`%LOCALAPPDATA%\KnowGiver` on Windows); `KG_DATA_DIR` overrides it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub data_dir: PathBuf,
}

impl AppPaths {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    /// Standard location for this user.
    pub fn detect() -> Self {
        if let Some(dir) = std::env::var_os("KG_DATA_DIR").filter(|d| !d.is_empty()) {
            return Self::new(dir);
        }
        let base = directories::BaseDirs::new()
            .map(|d| d.data_local_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        Self::new(base.join("KnowGiver"))
    }

    pub fn database(&self) -> PathBuf {
        self.data_dir.join("know-giver.db")
    }

    /// Where generated handout PDFs are kept with the app's local data.
    pub fn handouts_dir(&self) -> PathBuf {
        self.data_dir.join("handouts")
    }

    /// Picks a non-existing path so a new handout never replaces an older one.
    pub(crate) fn available_handout_path(&self, file_name: &str) -> std::io::Result<PathBuf> {
        let directory = self.handouts_dir();
        let path = directory.join(file_name);
        if !path.exists() {
            return Ok(path);
        }

        let name = Path::new(file_name);
        let stem = name.file_stem().unwrap_or_default().to_string_lossy();
        let extension = name.extension().map(|ext| ext.to_string_lossy());
        let mut suffix = 2u64;
        loop {
            let file_name = match &extension {
                Some(extension) => format!("{stem}-{suffix}.{extension}"),
                None => format!("{stem}-{suffix}"),
            };
            let candidate = directory.join(file_name);
            if !candidate.exists() {
                return Ok(candidate);
            }
            suffix = suffix.checked_add(1).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "no unique handout filename is available",
                )
            })?;
        }
    }

    /// The latest lesson recording, overwritten each time.
    pub fn last_recording(&self) -> PathBuf {
        self.data_dir.join("last-recording.wav")
    }

    /// Log of the current run, replaced at each start.
    pub fn log_file(&self) -> PathBuf {
        self.data_dir.join("know-giver.log")
    }

    pub fn models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
    }

    pub(crate) fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.models_dir())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handouts_are_kept_under_app_data() {
        let paths = AppPaths::new("app-data");
        assert_eq!(
            paths.handouts_dir(),
            PathBuf::from("app-data").join("handouts")
        );
    }

    #[test]
    fn new_handout_path_does_not_replace_an_existing_pdf() {
        let directory = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(directory.path());
        std::fs::create_dir_all(paths.handouts_dir()).unwrap();
        std::fs::write(paths.handouts_dir().join("handout-crase.pdf"), b"").unwrap();

        assert_eq!(
            paths.available_handout_path("handout-crase.pdf").unwrap(),
            paths.handouts_dir().join("handout-crase-2.pdf")
        );
    }
}
