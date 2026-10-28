//! Local PDF extraction with an opt-in vision fallback for scanned documents.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use kg_pdf::{Edital, EditalText, PdfError};

use crate::EngineError;

pub(crate) fn extract_document(
    client: &kg_ai::Client,
    path: &Path,
    cancel: &AtomicBool,
) -> Result<Edital, EngineError> {
    match kg_pdf::extract(path) {
        Ok(edital) => return Ok(edital),
        Err(PdfError::NoText(_)) if !client.config().models.vision.trim().is_empty() => {}
        Err(err) => return Err(err.into()),
    }
    let mut text = String::new();
    let pages = kg_pdf::read_scanned_pages(path, |page, total, png| -> Result<(), EngineError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(EngineError::Cancelled);
        }
        tracing::info!(page, total, "transcribing scanned PDF page");
        let transcript = client.transcribe_page(png)?;
        if cancel.load(Ordering::Relaxed) {
            return Err(EngineError::Cancelled);
        }
        text.push_str(&transcript);
        text.push_str("\n\n");
        Ok(())
    })?;
    let text = EditalText::from_raw(&text);
    if text.as_str().replace("[sem texto]", "").trim().is_empty() {
        return Err(PdfError::NoText(path.into()).into());
    }
    Ok(Edital {
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        pages,
        text,
    })
}
