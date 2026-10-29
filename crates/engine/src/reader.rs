//! Reading generated handouts inside the app: their pages, how far the
//! student read, and their highlights and notes.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kg_handout::{Document, Mark, PageImage, Pages};
use kg_study::{Highlight, HighlightColor, HighlightId, Reading, ReadingId};

use crate::{Engine, EngineError, Event, Notice};

/// Page images kept rendered; the farthest from the current page go first.
const KEPT_PAGES: usize = 8;
/// Progress is saved at most this often while reading.
const SAVE_EVERY: Duration = Duration::from_secs(3);

/// A handout open in the reader.
pub struct ReaderState {
    pub reading: Reading,
    /// Laid-out pages; `None` while they are being prepared.
    pub pages: Option<Arc<Pages>>,
    /// Laying the pages out failed.
    pub failed: bool,
    pub highlights: Vec<Highlight>,
    /// The handout's content with its pictures, for exporting.
    document: Option<Arc<Document>>,
    images: HashMap<(usize, u32), Arc<PageImage>>,
    pending: HashSet<(usize, u32)>,
    unsaved_since: Option<Instant>,
}

impl std::fmt::Debug for ReaderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReaderState")
            .field("reading", &self.reading.id)
            .field("pages", &self.pages.as_ref().map(|p| p.len()))
            .finish_non_exhaustive()
    }
}

impl ReaderState {
    /// A page drawn at `scale` (pixels per point × 100), once rendered.
    pub fn page_image(&self, page: usize, scale: u32) -> Option<&Arc<PageImage>> {
        self.images.get(&(page, scale))
    }

    /// Text boxes of the highlight, one rectangle per line (points).
    pub fn highlight_rects(&self, highlight: &Highlight) -> Vec<[f32; 4]> {
        self.pages.as_ref().map_or_else(Vec::new, |pages| {
            pages.line_rects(highlight.page, highlight.first, highlight.last)
        })
    }
}

/// What a background reader job sends back.
pub(crate) enum ReaderEvent {
    Ready {
        reading: ReadingId,
        result: Result<(Arc<Pages>, Arc<Document>), EngineError>,
    },
    Rendered {
        reading: ReadingId,
        page: usize,
        scale: u32,
        image: Option<PageImage>,
    },
    Exported(Result<PathBuf, EngineError>),
}

/// A handout that was just generated, to keep for reading.
#[derive(Debug, Clone)]
pub(crate) struct SavedHandout {
    pub path: PathBuf,
    pub title: String,
    /// The content as JSON, without its pictures (they are looked up again).
    pub document: String,
}

impl Engine {
    /// Handouts of the active plan kept for reading, newest first.
    pub fn readings(&self) -> &[Reading] {
        &self.readings
    }

    pub fn reader(&self) -> Option<&ReaderState> {
        self.reader.as_ref()
    }

    pub(crate) fn refresh_readings(&mut self) {
        self.readings = self
            .active
            .as_ref()
            .and_then(|plan| self.store.readings(plan.id).ok())
            .unwrap_or_default();
    }

    /// Keeps a generated handout; opens it when `open`.
    pub(crate) fn keep_handout(
        &mut self,
        saved: &SavedHandout,
        open: bool,
    ) -> Result<(), EngineError> {
        let plan = self.active.as_ref().ok_or(EngineError::NoActivePlan)?;
        let id = self.store.insert_reading(
            plan.id,
            &saved.title,
            &saved.path.display().to_string(),
            &saved.document,
            Self::today(),
        )?;
        self.refresh_readings();
        if open {
            self.open_reading(id)?;
        }
        Ok(())
    }

    /// Test hook: keeps `document` as a generated handout (its PDF goes to
    /// `dir`) and opens it in the reader.
    #[cfg(feature = "test-support")]
    pub fn simulate_handout(
        &mut self,
        document: Document,
        dir: &std::path::Path,
    ) -> Result<(), EngineError> {
        let saved = crate::handouts::saved_handout(document, &dir.join("apostila.pdf"), None)?;
        self.keep_handout(&saved, true)
    }

    pub(crate) fn open_reading(&mut self, id: ReadingId) -> Result<(), EngineError> {
        self.close_reader();
        let reading = self
            .readings
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or(EngineError::NoActivePlan)?;
        let json = self.store.reading_document(id)?.unwrap_or_default();
        let highlights = self.store.highlights(id)?;
        self.reader = Some(ReaderState {
            reading,
            pages: None,
            failed: false,
            highlights,
            document: None,
            images: HashMap::new(),
            pending: HashSet::new(),
            unsaved_since: None,
        });
        let pictures = self.picture_cache();
        self.spawn(move || {
            let result = (|| {
                let mut document: Document = serde_json::from_str(&json)
                    .map_err(|e| kg_handout::HandoutError::Layout(e.to_string()))?;
                if let Some(cache) = pictures {
                    crate::illustrations::illustrate(&mut document, &cache);
                }
                let pages = kg_handout::paginate(&document)?;
                Ok((Arc::new(pages), Arc::new(document)))
            })();
            Event::Reader(ReaderEvent::Ready {
                reading: id,
                result,
            })
        });
        Ok(())
    }

    pub(crate) fn close_reader(&mut self) {
        self.save_reading_progress();
        self.reader = None;
    }

    /// The page the student is looking at.
    pub(crate) fn reader_go_to(&mut self, page: usize) {
        let Some(reader) = self.reader.as_mut() else {
            return;
        };
        let reading = &mut reader.reading;
        let first_visit = reading.pages_seen.insert(page);
        if reading.last_page != page || first_visit {
            reading.last_page = page;
            reader.unsaved_since.get_or_insert_with(Instant::now);
        }
    }

    /// Draws a page in the background, unless it is drawn or on its way.
    pub(crate) fn render_page(&mut self, page: usize, scale: u32) {
        let Some(reader) = self.reader.as_mut() else {
            return;
        };
        let Some(pages) = reader.pages.clone() else {
            return;
        };
        let key = (page, scale);
        if page >= pages.len() || reader.images.contains_key(&key) || !reader.pending.insert(key) {
            return;
        }
        let reading = reader.reading.id;
        self.spawn(move || {
            let image = pages.render(page, scale as f32 / 100.0);
            Event::Reader(ReaderEvent::Rendered {
                reading,
                page,
                scale,
                image,
            })
        });
    }

    pub(crate) fn add_highlight(
        &mut self,
        page: usize,
        first: usize,
        last: usize,
        color: HighlightColor,
    ) -> Result<(), EngineError> {
        let Some(reader) = self.reader.as_mut() else {
            return Ok(());
        };
        let Some(pages) = &reader.pages else {
            return Ok(());
        };
        let (first, last) = (first.min(last), first.max(last));
        let text = pages.text_of(page, first..last + 1);
        if text.trim().is_empty() {
            return Ok(());
        }
        let mut highlight = Highlight {
            id: HighlightId(0),
            page,
            first,
            last,
            text,
            color,
            note: String::new(),
        };
        highlight.id = self
            .store
            .insert_highlight(reader.reading.id, &highlight, Self::today())?;
        reader.highlights.push(highlight);
        reader.highlights.sort_by_key(|h| (h.page, h.first));
        Ok(())
    }

    pub(crate) fn update_highlight(
        &mut self,
        id: HighlightId,
        color: HighlightColor,
        note: String,
    ) -> Result<(), EngineError> {
        self.store.update_highlight(id, color, note.trim())?;
        if let Some(h) = self
            .reader
            .as_mut()
            .and_then(|r| r.highlights.iter_mut().find(|h| h.id == id))
        {
            h.color = color;
            h.note = note.trim().to_owned();
        }
        Ok(())
    }

    pub(crate) fn delete_highlight(&mut self, id: HighlightId) -> Result<(), EngineError> {
        self.store.delete_highlight(id)?;
        if let Some(reader) = self.reader.as_mut() {
            reader.highlights.retain(|h| h.id != id);
        }
        Ok(())
    }

    pub(crate) fn delete_reading(&mut self, id: ReadingId) -> Result<(), EngineError> {
        if self.reader.as_ref().is_some_and(|r| r.reading.id == id) {
            self.reader = None;
        }
        self.store.delete_reading(id)?;
        self.refresh_readings();
        Ok(())
    }

    /// Saves the open handout as a PDF at `path`, with its highlights.
    pub(crate) fn export_reading(&mut self, path: PathBuf) {
        let Some(reader) = &self.reader else {
            return;
        };
        let (Some(pages), Some(document)) = (reader.pages.clone(), reader.document.clone()) else {
            return;
        };
        let marks: Vec<Mark> = reader
            .highlights
            .iter()
            .map(|h| {
                let [r, g, b, a] = h.color.rgba();
                Mark {
                    page: h.page + 1,
                    rects: pages
                        .line_rects(h.page, h.first, h.last)
                        .into_iter()
                        .map(|[left, top, right, bottom]| [left, top, right - left, bottom - top])
                        .collect(),
                    color: format!("#{r:02X}{g:02X}{b:02X}{a:02X}"),
                }
            })
            .collect();
        self.spawn(move || {
            let mut document = (*document).clone();
            document.marks = marks;
            let result = kg_handout::render_pdf(&document)
                .map_err(EngineError::from)
                .and_then(|pdf| {
                    std::fs::write(&path, pdf).map_err(|source| EngineError::Export {
                        path: path.clone(),
                        source,
                    })?;
                    Ok(path)
                });
            Event::Reader(ReaderEvent::Exported(result))
        });
    }

    pub(crate) fn on_reader_event(&mut self, event: ReaderEvent) -> Result<(), EngineError> {
        match event {
            ReaderEvent::Ready { reading, result } => {
                let Some(reader) = self.reader.as_mut().filter(|r| r.reading.id == reading) else {
                    return Ok(());
                };
                match result {
                    Ok((pages, document)) => {
                        let count = pages.len();
                        reader.reading.page_count = count;
                        reader.reading.last_page =
                            reader.reading.last_page.min(count.saturating_sub(1));
                        reader.pages = Some(pages);
                        reader.document = Some(document);
                        reader.unsaved_since.get_or_insert_with(Instant::now);
                    }
                    Err(err) => {
                        reader.failed = true;
                        return Err(err);
                    }
                }
            }
            ReaderEvent::Rendered {
                reading,
                page,
                scale,
                image,
            } => {
                let Some(reader) = self.reader.as_mut().filter(|r| r.reading.id == reading) else {
                    return Ok(());
                };
                reader.pending.remove(&(page, scale));
                if let Some(image) = image {
                    reader.images.insert((page, scale), Arc::new(image));
                }
                // Keep the pages nearest to where the student is.
                let current = reader.reading.last_page;
                while reader.images.len() > KEPT_PAGES {
                    let farthest = reader
                        .images
                        .keys()
                        .max_by_key(|(p, _)| p.abs_diff(current))
                        .copied();
                    match farthest {
                        Some(key) => reader.images.remove(&key),
                        None => break,
                    };
                }
            }
            ReaderEvent::Exported(result) => {
                self.notices.push(Notice::Exported(result?));
            }
        }
        Ok(())
    }

    /// Saves reading progress now and then while reading.
    pub(crate) fn tick_reader(&mut self) {
        let due = self
            .reader
            .as_ref()
            .and_then(|r| r.unsaved_since)
            .is_some_and(|since| since.elapsed() >= SAVE_EVERY);
        if due {
            self.save_reading_progress();
        }
    }

    fn save_reading_progress(&mut self) {
        let Some(reader) = self.reader.as_mut() else {
            return;
        };
        if reader.unsaved_since.take().is_none() {
            return;
        }
        if let Err(err) = self.store.save_reading_progress(&reader.reading) {
            tracing::warn!(%err, "could not save reading progress");
        }
        let saved = reader.reading.clone();
        if let Some(listed) = self.readings.iter_mut().find(|r| r.id == saved.id) {
            *listed = saved;
        }
    }
}
