//! Edital (exam notice) text extraction.

use std::path::{Path, PathBuf};

mod ocr;
mod roles;
pub use ocr::read_scanned_pages;
pub use roles::Role;

/// Plain text extracted from an edital, with whitespace normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditalText(String);

/// Headings that usually open the syllabus section of a Brazilian edital.
/// Compared against accent-free uppercase text.
const SYLLABUS_MARKERS: &[&str] = &[
    "CONTEUDO PROGRAMATICO",
    "CONTEUDOS PROGRAMATICOS",
    "OBJETOS DE AVALIACAO",
    "CONHECIMENTOS BASICOS",
    "CONHECIMENTOS GERAIS",
];

/// How much of the edital's start to keep for context (exam name, roles).
const HEADER_CHARS: usize = 4_000;
/// Below this, a "syllabus" match is probably just a mention in the text.
const MIN_SYLLABUS_CHARS: usize = 1_500;

impl EditalText {
    /// Normalizes raw extractor output: trims each line and collapses runs of
    /// blank lines, which PDF extractors produce a lot of.
    pub fn from_raw(raw: &str) -> Self {
        let mut out = String::with_capacity(raw.len());
        let mut in_blank_run = false;
        for line in raw.lines().map(str::trim) {
            if line.is_empty() {
                if !in_blank_run && !out.is_empty() {
                    out.push('\n');
                }
                in_blank_run = true;
                continue;
            }
            in_blank_run = false;
            out.push_str(line);
            out.push('\n');
        }
        Self(out.trim_end().to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn char_count(&self) -> usize {
        self.0.chars().count()
    }

    /// The part of the edital worth sending to the model: the opening (exam
    /// name, roles) plus the syllabus section when one is found, capped at
    /// `max_chars`.
    pub fn for_prompt(&self, max_chars: usize) -> String {
        let text = self.as_str();
        let excerpt = match self.syllabus_start() {
            Some(start) if start > HEADER_CHARS => {
                let header = take_chars(text, HEADER_CHARS);
                format!("{header}\n[…]\n{}", &text[start..])
            }
            _ => text.to_owned(),
        };
        take_chars(&excerpt, max_chars).to_owned()
    }

    /// Byte offset of the first heading-like line that opens the syllabus.
    fn syllabus_start(&self) -> Option<usize> {
        let text = self.as_str();
        let mut offset = 0;
        for line in text.split_inclusive('\n') {
            let start = offset;
            offset += line.len();
            if !is_heading(line) {
                continue;
            }
            let folded = fold(line);
            if SYLLABUS_MARKERS.iter().any(|m| folded.contains(m))
                && text.len() - start >= MIN_SYLLABUS_CHARS
            {
                return Some(start);
            }
        }
        None
    }
}

/// A line written mostly in capitals, like section titles.
fn is_heading(line: &str) -> bool {
    let letters: Vec<char> = line.chars().filter(|c| c.is_alphabetic()).collect();
    letters.len() >= 8
        && letters.iter().filter(|c| c.is_uppercase()).count() * 10 >= letters.len() * 8
}

/// Uppercase without Portuguese accents.
fn fold(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'Á' | 'À' | 'Â' | 'Ã' => 'A',
            'é' | 'ê' | 'É' | 'Ê' => 'E',
            'í' | 'Í' => 'I',
            'ó' | 'ô' | 'õ' | 'Ó' | 'Ô' | 'Õ' => 'O',
            'ú' | 'ü' | 'Ú' | 'Ü' => 'U',
            'ç' | 'Ç' => 'C',
            other => other.to_ascii_uppercase(),
        })
        .collect()
}

fn take_chars(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

/// An edital read from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edital {
    pub file_name: String,
    pub pages: usize,
    pub text: EditalText,
}

/// Errors while reading an edital.
///
/// Messages are English and meant for logs; the UI shows translated text.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse PDF {path}: {reason}")]
    Parse { path: PathBuf, reason: String },
    #[error("PDF {0} has no extractable text (possibly scanned)")]
    NoText(PathBuf),
}

/// Reads an edital PDF and extracts its text.
pub fn extract(path: &Path) -> Result<Edital, PdfError> {
    let bytes = std::fs::read(path).map_err(|source| PdfError::Io {
        path: path.to_owned(),
        source,
    })?;

    // The extractor can panic on malformed files; treat that as a parse error.
    let result = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem_by_pages(&bytes));
    let pages = match result {
        Ok(Ok(pages)) => pages,
        Ok(Err(err)) => {
            return Err(PdfError::Parse {
                path: path.to_owned(),
                reason: err.to_string(),
            });
        }
        Err(_) => {
            return Err(PdfError::Parse {
                path: path.to_owned(),
                reason: "extractor panicked".into(),
            });
        }
    };

    let text = EditalText::from_raw(&pages.join("\n"));
    if text.char_count() < 200 {
        return Err(PdfError::NoText(path.to_owned()));
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    tracing::info!(file = %file_name, pages = pages.len(), chars = text.char_count(), "edital extracted");
    Ok(Edital {
        file_name,
        pages: pages.len(),
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_whitespace() {
        let text = EditalText::from_raw("  CONTEÚDO  \n\n\n\n  Português \nCrase\n\n");
        assert_eq!(text.as_str(), "CONTEÚDO\n\nPortuguês\nCrase");
    }

    #[test]
    fn blank_input_is_empty() {
        assert!(EditalText::from_raw(" \n \n").is_empty());
    }

    #[test]
    fn prompt_keeps_header_and_syllabus() {
        let filler = "Regras gerais do concurso e cronograma.\n".repeat(300);
        let syllabus = "Língua Portuguesa: crase, regência e concordância.\n".repeat(60);
        let raw = format!(
            "EDITAL Nº 1 – TRIBUNAL REGIONAL\n{filler}conforme o conteúdo programático do anexo\n\
             ANEXO II – CONTEÚDO PROGRAMÁTICO\n{syllabus}"
        );
        let text = EditalText::from_raw(&raw);
        let prompt = text.for_prompt(1_000_000);
        assert!(prompt.starts_with("EDITAL Nº 1"));
        assert!(prompt.contains("ANEXO II – CONTEÚDO PROGRAMÁTICO\nLíngua Portuguesa"));
        assert!(prompt.len() < text.as_str().len());
    }

    #[test]
    fn prompt_falls_back_to_whole_text_and_caps_length() {
        let text = EditalText::from_raw(&"Ação ".repeat(100));
        assert_eq!(text.for_prompt(10).chars().count(), 10);
        assert_eq!(text.for_prompt(10_000), text.as_str());
    }

    #[test]
    fn missing_file_is_io_error() {
        let err = extract(Path::new("does-not-exist.pdf")).unwrap_err();
        assert!(matches!(err, PdfError::Io { .. }));
    }
}
