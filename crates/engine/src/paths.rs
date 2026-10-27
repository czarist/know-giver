//! Where the app keeps its data.

use std::path::PathBuf;

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

    pub fn models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
    }

    pub(crate) fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.models_dir())
    }
}
