//! A handout laid out in pages, for reading inside the app: each page as an
//! image, and where every piece of text sits on it, so the reader can select
//! and highlight exactly the words under the pointer.

use std::ops::Range;

use typst::layout::{Abs, Frame, FrameItem, Point, Transform};
use typst_layout::PagedDocument;

use crate::{Document, HandoutError, join};

/// One glyph cluster on a page: its text and its box in points, from the
/// page's top-left corner (`[left, top, right, bottom]`).
#[derive(Debug, Clone, PartialEq)]
pub struct TextBox {
    pub text: String,
    pub rect: [f32; 4],
}

/// A page drawn as pixels: premultiplied RGBA, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The handout's pages and the text on each one.
pub struct Pages {
    doc: PagedDocument,
    text: Vec<Vec<TextBox>>,
}

impl std::fmt::Debug for Pages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pages").field("pages", &self.len()).finish()
    }
}

/// Lays `document` out, exactly as its PDF.
pub fn paginate(document: &Document) -> Result<Pages, HandoutError> {
    let doc = crate::layout(document)?;
    let text = doc
        .pages()
        .iter()
        .map(|page| {
            let mut boxes = Vec::new();
            collect(&page.frame, Transform::identity(), &mut boxes);
            boxes
        })
        .collect();
    Ok(Pages { doc, text })
}

impl Pages {
    pub fn len(&self) -> usize {
        self.doc.pages().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Page size in points.
    pub fn size(&self, page: usize) -> Option<[f32; 2]> {
        let size = self.doc.pages().get(page)?.frame.size();
        Some([size.x.to_pt() as f32, size.y.to_pt() as f32])
    }

    /// Draws a page at `pixels_per_pt` (2.0 is sharp on most screens).
    pub fn render(&self, page: usize, pixels_per_pt: f32) -> Option<PageImage> {
        let page = self.doc.pages().get(page)?;
        let options = typst_render::RenderOptions {
            pixel_per_pt: f64::from(pixels_per_pt.clamp(0.25, 6.0)).into(),
            ..Default::default()
        };
        let pixmap = typst_render::render(page, &options);
        Some(PageImage {
            width: pixmap.width(),
            height: pixmap.height(),
            rgba: pixmap.take(),
        })
    }

    /// The text on a page, in reading order.
    pub fn text(&self, page: usize) -> &[TextBox] {
        self.text.get(page).map_or(&[], Vec::as_slice)
    }

    /// The text of a run of boxes on a page, with a space where a line breaks.
    pub fn text_of(&self, page: usize, range: Range<usize>) -> String {
        let boxes = self.text(page);
        let range = range.start.min(boxes.len())..range.end.min(boxes.len());
        let mut out = String::new();
        let mut last_top: Option<f32> = None;
        for b in &boxes[range] {
            if let Some(top) = last_top
                && (b.rect[1] - top).abs() > 2.0
                && !out.ends_with(' ')
            {
                out.push(' ');
            }
            out.push_str(&b.text);
            last_top = Some(b.rect[1]);
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The PDF of these pages.
    pub fn pdf(&self) -> Result<Vec<u8>, HandoutError> {
        typst_pdf::pdf(&self.doc, &typst_pdf::PdfOptions::default())
            .map_err(|errors| HandoutError::Pdf(join(&errors)))
    }
}

/// Walks a frame, collecting glyph boxes in page coordinates.
fn collect(frame: &Frame, ts: Transform, out: &mut Vec<TextBox>) {
    for (pos, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                let ts = ts
                    .pre_concat(Transform::translate(pos.x, pos.y))
                    .pre_concat(group.transform);
                collect(&group.frame, ts, out);
            }
            FrameItem::Text(text) => {
                let size = text.size;
                let mut x = Abs::zero();
                for glyph in &text.glyphs {
                    let advance = glyph.x_advance.at(size);
                    let left = *pos + Point::new(x + glyph.x_offset.at(size), Abs::zero());
                    let piece = text.text.get(glyph.range()).unwrap_or_default();
                    // Ascender and descender of usual text faces.
                    let top_left = apply(ts, left + Point::new(Abs::zero(), -size * 0.8));
                    let bottom_right = apply(ts, left + Point::new(advance, size * 0.24));
                    out.push(TextBox {
                        text: piece.to_owned(),
                        rect: [top_left[0], top_left[1], bottom_right[0], bottom_right[1]],
                    });
                    x += advance;
                }
            }
            FrameItem::Shape(..)
            | FrameItem::Image(..)
            | FrameItem::Link(..)
            | FrameItem::Tag(_) => {}
        }
    }
}

fn apply(ts: Transform, p: Point) -> [f32; 2] {
    let x = ts.sx.get() * p.x.to_pt() + ts.kx.get() * p.y.to_pt() + ts.tx.to_pt();
    let y = ts.ky.get() * p.x.to_pt() + ts.sy.get() * p.y.to_pt() + ts.ty.to_pt();
    [x as f32, y as f32]
}
