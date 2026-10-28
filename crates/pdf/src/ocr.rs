//! Bounded CPU rasterization for OCR, using the pure Rust hayro renderer.

use std::path::Path;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{RenderCache, RenderSettings};

use crate::PdfError;

/// Rasterizes one page at a time and passes its PNG to `read_page`.
///
/// The longest side is capped at 2,000 pixels. Pages are never silently skipped;
/// callback failures stop processing and propagate to the caller.
pub fn read_scanned_pages<E: From<PdfError>>(
    path: &Path,
    mut read_page: impl FnMut(usize, usize, &[u8]) -> Result<(), E>,
) -> Result<usize, E> {
    let bytes = std::fs::read(path).map_err(|source| PdfError::Io {
        path: path.into(),
        source,
    })?;
    let parse_error = |reason: String| PdfError::Parse {
        path: path.into(),
        reason,
    };
    let pdf =
        Pdf::new(bytes).map_err(|err| parse_error(format!("could not rasterize PDF: {err:?}")))?;
    let pages = pdf.pages();
    if pages.is_empty() {
        return Err(parse_error("PDF contains no pages".into()).into());
    }
    let cache = RenderCache::new();
    let interpreter = InterpreterSettings::default();
    for (index, page) in pages.iter().enumerate() {
        let (width, height) = page.render_dimensions();
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(parse_error(format!("invalid dimensions on page {}", index + 1)).into());
        }
        let scale = (2000.0 / width.max(height)).min(2.5);
        let settings = RenderSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
            ..Default::default()
        };
        let image = hayro::render(page, &cache, &interpreter, &settings);
        let png = image
            .into_png()
            .map_err(|err| parse_error(format!("could not encode page {}: {err}", index + 1)))?;
        read_page(index + 1, pages.len(), &png)?;
    }
    Ok(pages.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_pdf() -> Vec<u8> {
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1200 1600] /Resources << >> /Contents 4 0 R >>",
            "<< /Length 24 >>\nstream\n0 0 0 rg 10 10 30 30 re f\nendstream",
        ];
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = vec![0];
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
        }
        let xref = pdf.len();
        pdf.push_str("xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets.into_iter().skip(1) {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }

    #[test]
    fn renders_bounded_png_and_propagates_callback_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scanned.pdf");
        std::fs::write(&path, page_pdf()).unwrap();
        let mut called = false;
        let pages = read_scanned_pages(&path, |page, total, png| -> Result<(), PdfError> {
            assert_eq!((page, total), (1, 1));
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
            let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
            let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
            assert_eq!((width, height), (1500, 2000));
            called = true;
            Ok(())
        })
        .unwrap();
        assert!(called);
        assert_eq!(pages, 1);
        let error = read_scanned_pages(&path, |_, _, _| -> Result<(), PdfError> {
            Err(PdfError::NoText(path.clone()))
        })
        .unwrap_err();
        assert!(matches!(error, PdfError::NoText(_)));
    }
}
