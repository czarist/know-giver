//! Study handouts (apostilas) rendered to PDF with Typst, in pure Rust.
//!
//! The content is passed to the template as JSON data, never spliced into
//! markup, so model output can contain any character safely. Fonts are the
//! ones embedded in `typst-assets`, so no system fonts are needed.

use serde::{Deserialize, Serialize};
use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;

const TEMPLATE: &str = include_str!("../templates/handout.typ");

/// One section of a topic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Section {
    pub heading: String,
    #[serde(default)]
    pub paragraphs: Vec<String>,
    #[serde(default)]
    pub key_points: Vec<String>,
}

/// A practice question with its answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exercise {
    pub prompt: String,
    pub options: Vec<String>,
    pub correct: usize,
    #[serde(default)]
    pub explanation: String,
}

/// The written lesson for one topic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TopicHandout {
    pub topic: String,
    pub subject: String,
    #[serde(default)]
    pub introduction: String,
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub pitfalls: Vec<String>,
    #[serde(default)]
    pub summary: Vec<String>,
    #[serde(default)]
    pub exercises: Vec<Exercise>,
}

impl TopicHandout {
    /// Drops exercises the template cannot show (bad answer index, too many
    /// options), so one bad question never breaks the whole PDF.
    pub fn sanitized(mut self) -> Self {
        self.exercises
            .retain(|e| (2..=8).contains(&e.options.len()) && e.correct < e.options.len());
        self.sections.retain(|s| !s.heading.trim().is_empty());
        self
    }
}

/// Fixed words in the PDF, in the reader's language. They come from the
/// app's translation files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Labels {
    pub handout: String,
    pub contents: String,
    pub key_points: String,
    pub examples: String,
    pub pitfalls: String,
    pub summary: String,
    pub exercises: String,
    pub answers: String,
}

/// A whole handout: cover data plus one or more topics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Document {
    pub title: String,
    pub subtitle: String,
    /// Already formatted, e.g. "Gerado em 27/10/2026".
    pub generated: String,
    /// Typst language code for hyphenation, e.g. "pt" or "en".
    pub lang: String,
    pub labels: Labels,
    pub topics: Vec<TopicHandout>,
}

/// Rendering errors.
///
/// Messages are English and meant for logs; the UI shows translated text.
#[derive(Debug, thiserror::Error)]
pub enum HandoutError {
    #[error("handout has no topics")]
    Empty,
    #[error("could not lay out the handout: {0}")]
    Layout(String),
    #[error("could not write the PDF: {0}")]
    Pdf(String),
}

/// Renders `document` to PDF bytes.
pub fn render_pdf(document: &Document) -> Result<Vec<u8>, HandoutError> {
    if document.topics.is_empty() {
        return Err(HandoutError::Empty);
    }
    let data = serde_json::to_vec(document).map_err(|e| HandoutError::Layout(e.to_string()))?;
    let world = HandoutWorld::new(data)?;
    let compiled = typst::compile::<PagedDocument>(&world).output;
    let paged = compiled.map_err(|errors| HandoutError::Layout(join(&errors)))?;
    typst_pdf::pdf(&paged, &typst_pdf::PdfOptions::default())
        .map_err(|errors| HandoutError::Pdf(join(&errors)))
}

fn join(errors: &[SourceDiagnostic]) -> String {
    errors
        .iter()
        .map(|e| e.message.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

/// The minimal Typst "world": one source file, one data file, embedded fonts.
struct HandoutWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    main: Source,
    data_id: FileId,
    data: Bytes,
}

impl HandoutWorld {
    fn new(data: Vec<u8>) -> Result<Self, HandoutError> {
        let file = |name: &str| {
            VirtualPath::new(name)
                .map(|path| FileId::new(RootedPath::new(VirtualRoot::Project, path)))
                .map_err(|e| HandoutError::Layout(format!("bad virtual path {name}: {e:?}")))
        };
        let mut fonts = FontStore::new();
        fonts.extend(typst_kit::fonts::embedded());
        Ok(Self {
            library: LazyHash::new(Library::default()),
            fonts,
            main: Source::new(file("/main.typ")?, TEMPLATE.to_owned()),
            data_id: file("/data.json")?,
            data: Bytes::new(data),
        })
    }
}

impl World for HandoutWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.data_id {
            Ok(self.data.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> Labels {
        Labels {
            handout: "Apostila".into(),
            contents: "Sumário".into(),
            key_points: "Pontos-chave".into(),
            examples: "Exemplos".into(),
            pitfalls: "Pegadinhas".into(),
            summary: "Resumo".into(),
            exercises: "Exercícios".into(),
            answers: "Gabarito".into(),
        }
    }

    fn topic() -> TopicHandout {
        TopicHandout {
            topic: "Crase".into(),
            subject: "Língua Portuguesa".into(),
            // Characters that would be markup in Typst must come out as text.
            introduction: "A crase (à) é a fusão: a + a. Veja #isso, $x$, *negrito* e @ref.".into(),
            sections: vec![Section {
                heading: "Quando usar".into(),
                paragraphs: vec!["Antes de palavras femininas.".into()],
                key_points: vec!["Nunca antes de verbo.".into()],
            }],
            examples: vec!["Vou à escola.".into()],
            pitfalls: vec!["Não há crase antes de \"a partir\".".into()],
            summary: vec!["Preposição + artigo.".into()],
            exercises: vec![
                Exercise {
                    prompt: "Assinale a correta.".into(),
                    options: vec!["Vou à pé.".into(), "Vou à praia.".into()],
                    correct: 1,
                    explanation: "Praia é feminina.".into(),
                },
                Exercise {
                    prompt: "Inválida".into(),
                    options: vec!["a".into()],
                    correct: 5,
                    explanation: String::new(),
                },
            ],
        }
    }

    #[test]
    fn renders_a_pdf() {
        let document = Document {
            title: "Plano TJCE".into(),
            subtitle: "Língua Portuguesa".into(),
            generated: "Gerado em 27/10/2026".into(),
            lang: "pt".into(),
            labels: labels(),
            topics: vec![topic().sanitized()],
        };
        let pdf = render_pdf(&document).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        assert!(pdf.len() > 10_000);
        if let Some(dir) = std::env::var_os("KG_SNAPSHOT_DIR") {
            std::fs::write(std::path::Path::new(&dir).join("apostila.pdf"), &pdf).unwrap();
        }
    }

    #[test]
    fn sanitize_drops_broken_exercises() {
        assert_eq!(topic().sanitized().exercises.len(), 1);
    }

    #[test]
    fn empty_document_is_an_error() {
        let document = Document {
            title: String::new(),
            subtitle: String::new(),
            generated: String::new(),
            lang: "pt".into(),
            labels: labels(),
            topics: vec![],
        };
        assert!(matches!(render_pdf(&document), Err(HandoutError::Empty)));
    }
}
