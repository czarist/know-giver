//! Printable mock exams: question booklet, essay sheet and answer card,
//! laid out with Typst like the handouts.

use serde::{Deserialize, Serialize};
use typst_layout::PagedDocument;

use crate::{HandoutError, HandoutWorld, PageImage, join};

const TEMPLATE: &str = include_str!("../templates/exam.typ");

/// Questions the answer card holds: 5 columns of 40.
pub const CARD_CAPACITY: u32 = 200;

/// Where everything sits on the answer card, in millimetres from the top
/// left of an A4 page. Reading a photo of the card relies on these numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardLayout {
    /// Printed on every page and on the card, to match a photo to its exam.
    pub code: String,
    /// Gap between the page edge and each corner marker.
    pub margin: f32,
    /// Side of the square corner markers.
    pub marker: f32,
    /// Centre of the first row.
    pub top: f32,
    /// Left edge of the first column.
    pub left: f32,
    pub rows: u32,
    pub row_pitch: f32,
    pub column_width: f32,
    /// Room for the question number before the first bubble.
    pub number_width: f32,
    pub first_gap: f32,
    pub bubble: f32,
    pub bubble_pitch: f32,
}

impl CardLayout {
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            margin: 8.0,
            marker: 7.0,
            top: 62.0,
            left: 14.0,
            rows: 40,
            row_pitch: 5.4,
            column_width: 37.0,
            number_width: 7.0,
            first_gap: 4.0,
            bubble: 4.0,
            bubble_pitch: 5.6,
        }
    }

    /// Centre of the bubble for `mark` (0-based) of question `index`
    /// (0-based), in millimetres.
    pub fn bubble_centre(&self, index: u32, mark: u32) -> [f32; 2] {
        let column = index / self.rows;
        let row = index % self.rows;
        let x = self.left + column as f32 * self.column_width;
        [
            x + self.number_width + self.first_gap + mark as f32 * self.bubble_pitch,
            self.top + row as f32 * self.row_pitch,
        ]
    }
}

/// Fixed words in the PDF, from the app's translation files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ExamLabels {
    pub booklet: String,
    pub duration: String,
    pub questions: String,
    pub code: String,
    pub name: String,
    pub instructions: String,
    pub essay: String,
    pub essay_sheet: String,
    pub answer_card: String,
    pub card_hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExamPageQuestion {
    pub number: u32,
    pub prompt: String,
    /// Empty for true/false statements.
    pub options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExamPageSection {
    pub subject: String,
    pub questions: Vec<ExamPageQuestion>,
}

/// The essay proposal and how many lines the answer sheet has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EssaySheet {
    pub theme: String,
    pub statement: String,
    pub instructions: String,
    pub lines: u32,
}

/// A whole printable mock exam.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExamDocument {
    pub title: String,
    pub subtitle: String,
    /// Typst language code for hyphenation, e.g. "pt" or "en".
    pub lang: String,
    pub labels: ExamLabels,
    /// Already formatted, e.g. "4 h 30 min".
    pub duration: String,
    pub instructions: Vec<String>,
    pub sections: Vec<ExamPageSection>,
    /// Letters on the answer card and before options: "A", "B"… or "C", "E".
    pub marks: Vec<String>,
    pub total: u32,
    pub essay: Option<EssaySheet>,
    pub card: CardLayout,
}

impl ExamDocument {
    fn check(&self) -> Result<(), HandoutError> {
        let questions: usize = self.sections.iter().map(|s| s.questions.len()).sum();
        if questions == 0 {
            return Err(HandoutError::Empty);
        }
        if questions != self.total as usize || self.total > CARD_CAPACITY {
            return Err(HandoutError::Layout(
                "question count does not fit the card".into(),
            ));
        }
        let widest = self
            .sections
            .iter()
            .flat_map(|s| &s.questions)
            .map(|q| q.options.len())
            .max()
            .unwrap_or(0);
        if !(2..=5).contains(&self.marks.len()) || widest > self.marks.len() {
            return Err(HandoutError::Layout("bad answer marks".into()));
        }
        Ok(())
    }
}

fn layout(document: &ExamDocument) -> Result<PagedDocument, HandoutError> {
    document.check()?;
    let data = serde_json::to_vec(document).map_err(|e| HandoutError::Layout(e.to_string()))?;
    let world = HandoutWorld::new(TEMPLATE, data, &[], &document.lang)?;
    typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|errors| HandoutError::Layout(join(&errors)))
}

/// Renders the exam to PDF bytes.
pub fn render_exam_pdf(document: &ExamDocument) -> Result<Vec<u8>, HandoutError> {
    let paged = layout(document)?;
    typst_pdf::pdf(&paged, &typst_pdf::PdfOptions::default())
        .map_err(|errors| HandoutError::Pdf(join(&errors)))
}

/// Draws every page of the exam, for previews and tests.
pub fn render_exam_pages(
    document: &ExamDocument,
    pixels_per_pt: f32,
) -> Result<Vec<PageImage>, HandoutError> {
    let paged = layout(document)?;
    let options = typst_render::RenderOptions {
        pixel_per_pt: f64::from(pixels_per_pt.clamp(0.25, 6.0)).into(),
        ..Default::default()
    };
    Ok(paged
        .pages()
        .iter()
        .map(|page| {
            let pixmap = typst_render::render(page, &options);
            PageImage {
                width: pixmap.width(),
                height: pixmap.height(),
                rgba: pixmap.take(),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(total: u32, essay: bool) -> ExamDocument {
        let questions = (1..=total)
            .map(|n| ExamPageQuestion {
                number: n,
                prompt: format!("Assinale a alternativa correta sobre o item {n} #[x] $y$ *z*."),
                options: (0..5).map(|i| format!("Alternativa {i}")).collect(),
            })
            .collect();
        ExamDocument {
            title: "Simulado 1".into(),
            subtitle: "TJCE — Técnico Judiciário".into(),
            lang: "pt-BR".into(),
            labels: ExamLabels {
                booklet: "Caderno de prova".into(),
                duration: "Duração".into(),
                questions: "Questões".into(),
                code: "Código".into(),
                name: "Nome".into(),
                instructions: "Instruções".into(),
                essay: "Redação".into(),
                essay_sheet: "Folha de resposta".into(),
                answer_card: "Cartão-resposta".into(),
                card_hint: "Preencha a bolinha inteira com caneta preta.".into(),
            },
            duration: "4 h".into(),
            instructions: vec!["Marque uma só alternativa por questão.".into()],
            sections: vec![ExamPageSection {
                subject: "Língua Portuguesa".into(),
                questions,
            }],
            marks: ["A", "B", "C", "D", "E"].map(String::from).to_vec(),
            total,
            essay: essay.then(|| EssaySheet {
                theme: "Participação cidadã".into(),
                statement: "Texto de apoio.\nSegundo parágrafo.".into(),
                instructions: "Entre 20 e 30 linhas.".into(),
                lines: 30,
            }),
            card: CardLayout::new("KG-1-7"),
        }
    }

    #[test]
    fn renders_booklet_essay_and_card_to_pdf() {
        let pdf = render_exam_pdf(&document(12, true)).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn the_answer_card_is_the_last_page_and_a4() {
        let pages = render_exam_pages(&document(200, false), 1.0).unwrap();
        let card = pages.last().unwrap();
        // A4 at 1 px/pt: 595 × 842.
        assert!((594..=596).contains(&card.width) && (841..=843).contains(&card.height));
    }

    #[test]
    fn rejects_more_questions_than_the_card_holds() {
        assert!(render_exam_pdf(&document(201, false)).is_err());
    }

    #[test]
    fn bubbles_stay_inside_the_corner_markers() {
        let card = CardLayout::new("x");
        let inner_right = 210.0 - card.margin - card.marker;
        let inner_bottom = 297.0 - card.margin - card.marker;
        let [x, y] = card.bubble_centre(CARD_CAPACITY - 1, 4);
        assert!(x + card.bubble / 2.0 < inner_right + card.marker);
        assert!(y + card.bubble / 2.0 < inner_bottom);
        let [x0, y0] = card.bubble_centre(0, 0);
        assert!(x0 > card.margin + card.marker && y0 > 45.0);
    }

    /// Writes PNGs of every page to `KG_EXAM_PNG` to look at the layout.
    #[test]
    #[ignore = "writes files; run with --ignored"]
    fn exam_pages_png() {
        let Some(dir) = std::env::var_os("KG_EXAM_PNG") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let paged = layout(&document(60, true)).unwrap();
        let options = typst_render::RenderOptions {
            pixel_per_pt: 1.5.into(),
            ..Default::default()
        };
        for (i, page) in paged.pages().iter().enumerate() {
            let png = typst_render::render(page, &options).encode_png().unwrap();
            std::fs::write(dir.join(format!("exam-{i}.png")), png).unwrap();
        }
    }
}
