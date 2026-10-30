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

mod cjk;
mod pages;

pub use cjk::{CjkFont, CjkScript, cjk_font};
pub use pages::{PageImage, Pages, TextBox, paginate};

const TEMPLATE: &str = include_str!("../templates/handout.typ");

/// A picture placed in the handout: a file of [`Document::files`] and its
/// caption (where it comes from).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Picture {
    /// Path the template loads, e.g. "/img/3.jpg".
    pub file: String,
    pub caption: String,
}

/// One section of a topic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Section {
    pub heading: String,
    #[serde(default)]
    pub paragraphs: Vec<String>,
    #[serde(default)]
    pub key_points: Vec<String>,
    /// What to show a picture of (a Wikipedia article title); may be empty.
    #[serde(default)]
    pub image: String,
    /// The picture found for `image`, set just before rendering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<Picture>,
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
    /// A picture of the topic itself, set just before rendering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<Picture>,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Labels {
    pub handout: String,
    pub contents: String,
    pub key_points: String,
    pub examples: String,
    pub pitfalls: String,
    pub summary: String,
    pub exercises: String,
    pub answers: String,
    /// Start of a picture caption, e.g. "Imagem: Wikipédia".
    pub picture_source: String,
    /// Heading of the quick answer grid, e.g. "Respostas".
    pub answer_grid: String,
}

/// A highlight drawn over the text when exporting: rectangles in points
/// (`[x, y, width, height]` from the page's top-left) on a 1-based page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    pub page: usize,
    pub rects: Vec<[f32; 4]>,
    /// Translucent color as "#RRGGBBAA".
    pub color: String,
}

/// A whole handout: cover data plus one or more topics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub title: String,
    pub subtitle: String,
    /// Already formatted, e.g. "Gerado em 27/10/2026".
    pub generated: String,
    /// Typst language code for hyphenation, e.g. "pt" or "en".
    pub lang: String,
    pub labels: Labels,
    pub topics: Vec<TopicHandout>,
    /// Picture files by the path the template loads them from. Not saved
    /// with the document: they are looked up again (from the disk cache).
    #[serde(skip)]
    pub files: Vec<(String, Vec<u8>)>,
    /// The reader's highlights, drawn over the pages.
    #[serde(default)]
    pub marks: Vec<Mark>,
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
    let paged = layout(document)?;
    typst_pdf::pdf(&paged, &typst_pdf::PdfOptions::default())
        .map_err(|errors| HandoutError::Pdf(join(&errors)))
}

/// Lays `document` out in pages, as the PDF has them.
pub(crate) fn layout(document: &Document) -> Result<PagedDocument, HandoutError> {
    if document.topics.is_empty() {
        return Err(HandoutError::Empty);
    }
    let data = serde_json::to_vec(document).map_err(|e| HandoutError::Layout(e.to_string()))?;
    let world = HandoutWorld::new(data, &document.files, &document.lang)?;
    let compiled = typst::compile::<PagedDocument>(&world).output;
    compiled.map_err(|errors| HandoutError::Layout(join(&errors)))
}

pub(crate) fn join(errors: &[SourceDiagnostic]) -> String {
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
    files: Vec<(FileId, Bytes)>,
}

impl HandoutWorld {
    fn new(data: Vec<u8>, files: &[(String, Vec<u8>)], lang: &str) -> Result<Self, HandoutError> {
        let file = |name: &str| {
            VirtualPath::new(name)
                .map(|path| FileId::new(RootedPath::new(VirtualRoot::Project, path)))
                .map_err(|e| HandoutError::Layout(format!("bad virtual path {name}: {e:?}")))
        };
        let mut fonts = FontStore::new();
        fonts.extend(typst_kit::fonts::embedded());
        // Chinese and Japanese handouts use the system's fonts for those
        // scripts, the handout's own script first.
        if let Some(first) = CjkScript::of_locale(lang)
            .or_else(|| lang.starts_with("zh").then_some(CjkScript::Simplified))
        {
            let order =
                std::iter::once(first).chain(CjkScript::ALL.into_iter().filter(|s| *s != first));
            for script in order {
                if let Some(font) = cjk_font(script) {
                    fonts.extend(Font::iter(Bytes::new(font.bytes.as_slice())).map(|font| {
                        let info = font.info().clone();
                        (font, info)
                    }));
                }
            }
        }
        Ok(Self {
            library: LazyHash::new(Library::default()),
            fonts,
            main: Source::new(file("/main.typ")?, TEMPLATE.to_owned()),
            data_id: file("/data.json")?,
            data: Bytes::new(data),
            files: files
                .iter()
                .map(|(path, bytes)| Ok((file(path)?, Bytes::new(bytes.clone()))))
                .collect::<Result<_, HandoutError>>()?,
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
        } else if let Some((_, bytes)) = self.files.iter().find(|(file, _)| *file == id) {
            Ok(bytes.clone())
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
            picture_source: "Imagem: Wikipédia".into(),
            answer_grid: "Respostas".into(),
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
                image: "Escola".into(),
                picture: Some(Picture {
                    file: "/img/1.svg".into(),
                    caption: "Imagem: Wikipédia · Escola".into(),
                }),
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
            picture: None,
        }
    }

    /// A tiny picture, so figures can be laid out without the network.
    const PICTURE: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='40' height='30'><rect width='40' height='30' fill='teal'/></svg>";

    #[test]
    fn renders_a_pdf() {
        let document = Document {
            title: "Plano TJCE".into(),
            subtitle: "Língua Portuguesa".into(),
            generated: "Gerado em 27/10/2026".into(),
            lang: "pt".into(),
            labels: labels(),
            topics: vec![topic().sanitized()],
            files: vec![("/img/1.svg".into(), PICTURE.as_bytes().to_vec())],
            marks: Vec::new(),
        };
        let pdf = render_pdf(&document).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        assert!(pdf.len() > 10_000);

        // The same pages for the reader: pictures, and text with positions.
        let pages = paginate(&document).unwrap();
        assert!(pages.len() >= 3);
        let found = (0..pages.len()).find_map(|page| {
            let boxes = pages.text(page);
            let all = pages.text_of(page, 0..boxes.len());
            all.contains("Antes de palavras femininas")
                .then_some((page, boxes.len()))
        });
        let (page, count) = found.expect("section text is on some page");
        let [width, height] = pages.size(page).unwrap();
        assert!(
            pages
                .text(page)
                .iter()
                .all(|b| b.rect[0] >= 0.0 && b.rect[2] <= width && b.rect[3] <= height)
        );
        assert!(count > 20);
        let image = pages.render(page, 1.0).unwrap();
        assert_eq!(image.rgba.len(), (image.width * image.height * 4) as usize);

        // A highlight becomes one rectangle per line, and is drawn in the PDF.
        let lines = pages.line_rects(page, 0, count - 1);
        assert!(!lines.is_empty() && lines.len() < count);
        let mut marked = document.clone();
        marked.marks.push(Mark {
            page: page + 1,
            rects: lines
                .iter()
                .map(|[l, t, r, b]| [*l, *t, r - l, b - t])
                .collect(),
            color: "#F5C54270".into(),
        });
        assert_ne!(render_pdf(&marked).unwrap(), pdf);
        if let Some(dir) = std::env::var_os("KG_SNAPSHOT_DIR") {
            let marked_pages = paginate(&marked).unwrap();
            let image = marked_pages.render(page, 1.5).unwrap();
            let rgb: Vec<u8> = image
                .rgba
                .chunks(4)
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect();
            let mut ppm = format!(
                "P6
{} {}
255
",
                image.width, image.height
            )
            .into_bytes();
            ppm.extend(rgb);
            std::fs::write(std::path::Path::new(&dir).join("apostila-marked.ppm"), ppm).unwrap();
        }

        if let Some(dir) = std::env::var_os("KG_SNAPSHOT_DIR") {
            let dir = std::path::Path::new(&dir);
            std::fs::write(dir.join("apostila.pdf"), &pdf).unwrap();
            for page in 0..pages.len() {
                let image = pages.render(page, 1.5).unwrap();
                let rgb: Vec<u8> = image
                    .rgba
                    .chunks(4)
                    .flat_map(|p| [p[0], p[1], p[2]])
                    .collect();
                let header = format!("P6\n{} {}\n255\n", image.width, image.height);
                let mut ppm = header.into_bytes();
                ppm.extend(rgb);
                std::fs::write(dir.join(format!("apostila-{page}.ppm")), ppm).unwrap();
            }
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
            files: Vec::new(),
            marks: Vec::new(),
        };
        assert!(matches!(render_pdf(&document), Err(HandoutError::Empty)));
    }
}
