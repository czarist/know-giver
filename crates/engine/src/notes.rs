//! The student's preference notes: a Markdown file the student can edit and
//! the tutor adds to (`PREFERENCIA:` lines), given to the tutor at the start
//! of each lesson so lessons follow how the student likes to learn. Off with
//! [`crate::Settings::student_notes`].

use std::fs;
use std::path::PathBuf;

use crate::{AppPaths, Engine, EngineError};

/// Folder of the notes under the data folder; it travels with backups.
pub(crate) const NOTES_DIR: &str = "notes";
const NOTES_FILE: &str = "student-notes.md";

impl AppPaths {
    /// The student's preference notes (Markdown, editable).
    pub fn student_notes(&self) -> PathBuf {
        self.data_dir.join(NOTES_DIR).join(NOTES_FILE)
    }
}

/// The notes, or empty when there are none yet.
pub(crate) fn read(paths: &AppPaths) -> String {
    fs::read_to_string(paths.student_notes()).unwrap_or_default()
}

fn write(paths: &AppPaths, text: &str) -> Result<(), EngineError> {
    let path = paths.student_notes();
    let io = |source| EngineError::Export {
        path: path.clone(),
        source,
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io)?;
    }
    fs::write(&path, text).map_err(io)
}

/// `notes` with each new line added at the end as a dated item; lines it
/// already holds are left out.
fn with_added(notes: &str, added: &[String], date: chrono::NaiveDate) -> Option<String> {
    let fresh: Vec<&String> = added
        .iter()
        .filter(|note| !note.trim().is_empty() && !notes.contains(note.trim()))
        .collect();
    if fresh.is_empty() {
        return None;
    }
    let mut out = notes.trim_end().to_owned();
    if !out.is_empty() {
        out.push('\n');
    }
    for note in fresh {
        out.push_str(&format!("- {date}: {}\n", note.trim()));
    }
    Some(out)
}

impl Engine {
    /// The notes as saved, for the editor in Settings.
    pub fn student_notes(&self) -> &str {
        &self.student_notes
    }

    /// Saves the notes as the student edited them.
    pub(crate) fn save_student_notes(&mut self, text: String) -> Result<(), EngineError> {
        write(&self.paths, &text)?;
        self.student_notes = text;
        Ok(())
    }

    /// Adds what the tutor noticed, when the student keeps notes. A failure
    /// only costs the note.
    pub(crate) fn add_student_notes(&mut self, added: &[String]) {
        if !self.settings.student_notes {
            return;
        }
        // From the file: the student may have edited it in another app.
        let current = read(&self.paths);
        let Some(text) = with_added(&current, added, Self::today()) else {
            return;
        };
        match write(&self.paths, &text) {
            Ok(()) => {
                tracing::info!(count = added.len(), "tutor added to the student's notes");
                self.student_notes = text;
            }
            Err(err) => tracing::warn!(%err, "could not save the student's notes"),
        }
    }

    /// What the tutor is given, read from the file (the student may have
    /// edited it in another app): `None` when the notes are off.
    pub(crate) fn notes_for_tutor(&self) -> Option<String> {
        self.settings.student_notes.then(|| read(&self.paths))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_added_once_with_their_date() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 11, 28).unwrap();
        let first = with_added(
            "# Eu\nGosto de exemplos.",
            &["prefere ir devagar".into()],
            date,
        )
        .unwrap();
        assert_eq!(
            first,
            "# Eu\nGosto de exemplos.\n- 2026-11-28: prefere ir devagar\n"
        );
        assert_eq!(
            with_added(&first, &["prefere ir devagar".into()], date),
            None
        );
        assert_eq!(
            with_added("", &["usa fone".into()], date).unwrap(),
            "- 2026-11-28: usa fone\n"
        );
    }
}
