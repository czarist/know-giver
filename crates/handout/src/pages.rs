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

    /// The text of a run of boxes on a page, with a space where a line
    /// breaks (none next to Chinese or Japanese text).
    pub fn text_of(&self, page: usize, range: Range<usize>) -> String {
        let boxes = self.text(page);
        let range = range.start.min(boxes.len())..range.end.min(boxes.len());
        join_boxes(boxes[range].iter().map(|b| (b.text.as_str(), b.rect[1])))
    }

    /// Boxes `first..=last` of a page joined into one rectangle per line
    /// (`[left, top, right, bottom]` in points), for drawing a highlight.
    pub fn line_rects(&self, page: usize, first: usize, last: usize) -> Vec<[f32; 4]> {
        let boxes = self.text(page);
        let last = last.min(boxes.len().saturating_sub(1));
        let mut lines: Vec<[f32; 4]> = Vec::new();
        for b in boxes.get(first..=last).unwrap_or_default() {
            match lines.last_mut() {
                // Same line: about the same top, continuing to the right.
                Some(line)
                    if (b.rect[1] - line[1]).abs() < 2.0
                        && b.rect[0] >= line[0] - 1.0
                        && b.rect[0] - line[2] < 12.0 =>
                {
                    line[2] = line[2].max(b.rect[2]);
                    line[3] = line[3].max(b.rect[3]);
                }
                _ => lines.push(b.rect),
            }
        }
        lines
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

/// Joins `(text, top)` boxes in reading order: a line break becomes a space,
/// except next to Chinese or Japanese characters, which take no spaces.
fn join_boxes<'a>(boxes: impl Iterator<Item = (&'a str, f32)>) -> String {
    let mut out = String::new();
    let mut last_top: Option<f32> = None;
    for (text, top) in boxes {
        let breaks = last_top.is_some_and(|last| (top - last).abs() > 2.0);
        if breaks {
            let cjk = out.trim_end().chars().last().is_some_and(is_cjk)
                || text.trim_start().chars().next().is_some_and(is_cjk);
            if cjk {
                out.truncate(out.trim_end().len());
                out.push_str(text.trim_start());
            } else {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
                out.push_str(text);
            }
        } else {
            out.push_str(text);
        }
        last_top = Some(top);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Han, kana and their full-width punctuation.
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{30FF}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{FF00}'..='\u{FFEF}')
}

#[cfg(test)]
mod tests {
    use super::join_boxes;

    #[test]
    fn line_breaks_become_spaces_except_in_chinese_and_japanese() {
        let latin = [("The list of", 10.0), ("names is long.", 24.0)];
        assert_eq!(join_boxes(latin.into_iter()), "The list of names is long.");
        let chinese = [("先去掉以 of 开头的短语", 10.0), ("，剩下的就是中心词", 24.0)];
        assert_eq!(
            join_boxes(chinese.into_iter()),
            "先去掉以 of 开头的短语，剩下的就是中心词"
        );
        let japanese = [("動詞は主語に", 10.0), ("一致します。", 24.0), ("the list", 38.0)];
        assert_eq!(
            join_boxes(japanese.into_iter()),
            "動詞は主語に一致します。the list"
        );
    }
}
