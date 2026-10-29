//! Photos of handwritten pages: checking what a file really is, keeping the
//! original, preparing a bounded JPEG for the model and spotting photos
//! that will be hard to read.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Limits};
use kg_study::{NewPage, PageIssue};

use crate::EngineError;

/// Largest photo file accepted.
pub(crate) const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;
/// Largest width or height accepted.
pub(crate) const MAX_SIDE: u32 = 12_000;
/// Below this short side, handwriting cannot be read: the page is refused.
pub(crate) const MIN_SHORT_SIDE: u32 = 700;
/// Long side of the JPEG sent to the model.
pub(crate) const PROCESSED_LONG_SIDE: u32 = 2_000;
/// Below this edge strength a page is called blurry. A crisp photo of ink
/// on paper measures well over 100; a Gaussian blur of 3 px about 40.
const BLUR_EDGE: usize = 50;
/// Long side the quality checks look at.
const CHECK_LONG_SIDE: u32 = 1_000;

/// Why a file cannot be used as a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProblem {
    /// Over [`MAX_FILE_BYTES`].
    TooLarge,
    /// Not a JPEG or PNG, whatever its name says.
    Unsupported,
    /// Wider or taller than [`MAX_SIDE`].
    TooManyPixels,
    /// A JPEG or PNG that could not be decoded.
    Unreadable,
}

/// A page ready to be saved, with the JPEG for the model.
pub(crate) struct PreparedPage {
    pub(crate) page: NewPage,
    pub(crate) jpeg: Vec<u8>,
}

/// Reads a photo the student picked, checking its size and real format.
pub(crate) fn read_photo(path: &Path) -> Result<(Vec<u8>, ImageFormat), EngineError> {
    let io = |source| EngineError::Export {
        path: path.to_path_buf(),
        source,
    };
    let size = std::fs::metadata(path).map_err(io)?.len();
    if size > MAX_FILE_BYTES {
        return Err(EngineError::Image(ImageProblem::TooLarge));
    }
    let bytes = std::fs::read(path).map_err(io)?;
    let format = image::guess_format(&bytes)
        .ok()
        .filter(|f| matches!(f, ImageFormat::Jpeg | ImageFormat::Png))
        .ok_or(EngineError::Image(ImageProblem::Unsupported))?;
    Ok((bytes, format))
}

/// Decodes a photo, applying its own orientation (EXIF) and then
/// `rotation` quarter turns clockwise.
pub(crate) fn decode(
    bytes: &[u8],
    format: ImageFormat,
    rotation: u8,
) -> Result<DynamicImage, EngineError> {
    let mut reader = ImageReader::new(Cursor::new(bytes));
    reader.set_format(format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(1024 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(image_error)?;
    let orientation = decoder.orientation().map_err(image_error)?;
    let mut img = DynamicImage::from_decoder(decoder).map_err(image_error)?;
    img.apply_orientation(orientation);
    Ok(match rotation % 4 {
        1 => img.rotate90(),
        2 => img.rotate180(),
        3 => img.rotate270(),
        _ => img,
    })
}

fn image_error(err: image::ImageError) -> EngineError {
    match err {
        image::ImageError::Limits(_) => EngineError::Image(ImageProblem::TooManyPixels),
        image::ImageError::Unsupported(_) => EngineError::Image(ImageProblem::Unsupported),
        _ => EngineError::Image(ImageProblem::Unreadable),
    }
}

/// Copies the original into `dir`, writes the prepared JPEG next to it and
/// checks the photo. The original is kept as it came.
pub(crate) fn store_photo(
    dir: &Path,
    bytes: &[u8],
    format: ImageFormat,
) -> Result<PreparedPage, EngineError> {
    let img = decode(bytes, format, 0)?;
    let extension = if format == ImageFormat::Png {
        "png"
    } else {
        "jpg"
    };
    let stamp = unique();
    std::fs::create_dir_all(dir).map_err(|source| EngineError::Export {
        path: dir.to_path_buf(),
        source,
    })?;
    let original = dir.join(format!("original-{stamp}.{extension}"));
    write(&original, bytes)?;
    prepare(dir, &original, &img, 0, &stamp)
}

/// Prepares the page again from its kept original with a new rotation.
pub(crate) fn rotate_photo(original: &Path, rotation: u8) -> Result<PreparedPage, EngineError> {
    let (bytes, format) = read_photo(original)?;
    let img = decode(&bytes, format, rotation)?;
    let dir = original.parent().unwrap_or(Path::new("."));
    prepare(dir, original, &img, rotation, &unique())
}

fn prepare(
    dir: &Path,
    original: &Path,
    img: &DynamicImage,
    rotation: u8,
    stamp: &str,
) -> Result<PreparedPage, EngineError> {
    let (width, height) = (img.width(), img.height());
    let mut issues = check(img);
    if width.min(height) < MIN_SHORT_SIDE {
        issues.insert(0, PageIssue::LowResolution);
    }
    let jpeg = encode_jpeg(&bounded(img, PROCESSED_LONG_SIDE))?;
    let processed = dir.join(format!("page-{stamp}.jpg"));
    write(&processed, &jpeg)?;
    Ok(PreparedPage {
        page: NewPage {
            original_path: original.to_string_lossy().into_owned(),
            processed_path: processed.to_string_lossy().into_owned(),
            rotation,
            width,
            height,
            issues,
        },
        jpeg,
    })
}

fn bounded(img: &DynamicImage, long_side: u32) -> DynamicImage {
    if img.width().max(img.height()) <= long_side {
        img.clone()
    } else {
        img.resize(long_side, long_side, FilterType::Triangle)
    }
}

fn encode_jpeg(img: &DynamicImage) -> Result<Vec<u8>, EngineError> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 85)
        .encode_image(&img.to_rgb8())
        .map_err(image_error)?;
    Ok(out)
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), EngineError> {
    std::fs::write(path, bytes).map_err(|source| EngineError::Export {
        path: path.to_path_buf(),
        source,
    })
}

/// A name part no other file of this run has.
fn unique() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Light, contrast and sharpness of a photo. These are rough signs that
/// reading may be hard, shown as warnings; the model has the last word.
/// They use percentiles so a page with little ink is not taken as blank.
pub(crate) fn check(img: &DynamicImage) -> Vec<PageIssue> {
    let gray = bounded(img, CHECK_LONG_SIDE).to_luma8();
    let pixels = gray.as_raw();
    if pixels.is_empty() {
        return vec![PageIssue::LowResolution];
    }
    let mut histogram = [0_usize; 256];
    for &v in pixels {
        histogram[usize::from(v)] += 1;
    }
    let n = pixels.len();
    let mean = pixels.iter().map(|&v| f64::from(v)).sum::<f64>() / n as f64;
    let dark = percentile(&histogram, n, 0.01);
    let light = percentile(&histogram, n, 0.99);
    let bright = histogram[250..].iter().sum::<usize>() as f64 / n as f64;
    let mut issues = Vec::new();
    if mean < 75.0 {
        issues.push(PageIssue::Dark);
    }
    if bright > 0.85 {
        issues.push(PageIssue::Overexposed);
    }
    if light.saturating_sub(dark) < 60 {
        issues.push(PageIssue::LowContrast);
    } else if edge_strength(&gray) < BLUR_EDGE {
        issues.push(PageIssue::Blurry);
    }
    issues
}

/// The value below which `share` of the pixels fall.
fn percentile(histogram: &[usize], total: usize, share: f64) -> usize {
    let target = (total as f64 * share).ceil() as usize;
    let mut seen = 0;
    for (value, count) in histogram.iter().enumerate() {
        seen += count;
        if seen >= target.max(1) {
            return value;
        }
    }
    histogram.len().saturating_sub(1)
}

/// How sharp the strongest edges are (99.5th percentile of the gradient):
/// ink on paper gives steep edges, a blurred photo soft ones.
fn edge_strength(gray: &image::GrayImage) -> usize {
    let (w, h) = gray.dimensions();
    if w < 3 || h < 3 {
        return 0;
    }
    let at = |x: u32, y: u32| i32::from(gray.get_pixel(x, y)[0]);
    let mut histogram = [0_usize; 512];
    let mut total = 0;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let gx = at(x + 1, y) - at(x - 1, y);
            let gy = at(x, y + 1) - at(x, y - 1);
            let magnitude = (gx.abs() + gy.abs()).min(511) as usize;
            histogram[magnitude] += 1;
            total += 1;
        }
    }
    percentile(&histogram, total, 0.995)
}

/// Digits drawn as 5×7 blocks, for checking that a model reads images.
const DIGITS: [[u8; 7]; 10] = [
    [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
    [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
    [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
    [0x1f, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0e],
    [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
    [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e],
    [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e],
    [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
    [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
    [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c],
];

/// A PNG with a random three-digit number, and the number.
pub(crate) fn vision_probe() -> Result<(Vec<u8>, String), EngineError> {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(4_271);
    let number = format!("{:03}", 100 + seed % 900);
    let scale = 14_u32;
    let (cell_w, cell_h) = (6 * scale, 9 * scale);
    let width = cell_w * 3 + 2 * scale;
    let height = cell_h + scale;
    let mut img = image::GrayImage::from_pixel(width, height, image::Luma([255]));
    for (i, digit) in number.bytes().enumerate() {
        let rows = DIGITS[usize::from(digit - b'0')];
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..5 {
                if bits & (0x10 >> col) != 0 {
                    let x0 = scale * 2 + i as u32 * cell_w + col * scale;
                    let y0 = scale + row as u32 * scale;
                    for y in y0..y0 + scale {
                        for x in x0..x0 + scale {
                            img.put_pixel(x, y, image::Luma([0]));
                        }
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(img.as_raw(), width, height, image::ExtendedColorType::L8)
        .map_err(image_error)?;
    Ok((out, number))
}

/// Folder with an attempt's photos.
pub(crate) fn attempt_dir(data_dir: &Path, attempt: kg_study::AttemptId) -> PathBuf {
    data_dir
        .join("discursive")
        .join(format!("attempt-{}", attempt.0))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A "page": white paper with dark handwriting-like strokes.
    pub(crate) fn page_png(width: u32, height: u32, paper: u8) -> Vec<u8> {
        let mut img = image::GrayImage::from_pixel(width, height, image::Luma([paper]));
        for line in 0..(height / 40) {
            let y = 20 + line * 40;
            for x in 30..width.saturating_sub(30) {
                if (x / 17 + line) % 5 != 0 {
                    for dy in 0..3 {
                        img.put_pixel(x, (y + dy + (x / 7) % 4).min(height - 1), image::Luma([20]));
                    }
                }
            }
        }
        let mut out = Vec::new();
        PngEncoder::new(&mut out)
            .write_image(img.as_raw(), width, height, image::ExtendedColorType::L8)
            .unwrap();
        out
    }

    #[test]
    fn files_are_checked_by_content_not_name() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("page.jpg");
        std::fs::write(&fake, b"%PDF-1.7 not an image").unwrap();
        assert!(matches!(
            read_photo(&fake),
            Err(EngineError::Image(ImageProblem::Unsupported))
        ));
        let real = dir.path().join("page.dat");
        std::fs::write(&real, page_png(900, 1200, 235)).unwrap();
        assert_eq!(read_photo(&real).unwrap().1, ImageFormat::Png);
    }

    #[test]
    fn originals_are_kept_and_processed_pages_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = page_png(2400, 3200, 235);
        let prepared = store_photo(dir.path(), &bytes, ImageFormat::Png).unwrap();
        assert_eq!(std::fs::read(&prepared.page.original_path).unwrap(), bytes);
        let processed = image::load_from_memory(&prepared.jpeg).unwrap();
        assert_eq!(processed.height(), PROCESSED_LONG_SIDE);
        assert_eq!((prepared.page.width, prepared.page.height), (2400, 3200));
        assert!(
            prepared.page.issues.is_empty(),
            "{:?}",
            prepared.page.issues
        );

        let turned = rotate_photo(Path::new(&prepared.page.original_path), 1).unwrap();
        assert_eq!((turned.page.width, turned.page.height), (3200, 2400));
        assert_eq!(turned.page.original_path, prepared.page.original_path);
    }

    #[test]
    fn hard_to_read_photos_are_flagged() {
        let dark = image::load_from_memory(&page_png(900, 1200, 40)).unwrap();
        assert!(check(&dark).contains(&PageIssue::Dark));
        let blank =
            DynamicImage::ImageLuma8(image::GrayImage::from_pixel(900, 1200, image::Luma([180])));
        assert!(check(&blank).contains(&PageIssue::LowContrast));
        let sharp = image::load_from_memory(&page_png(900, 1200, 235)).unwrap();
        assert!(check(&sharp).is_empty(), "{:?}", check(&sharp));
        // A soft photo is flagged; a very soft one also loses contrast.
        for sigma in [3.0, 6.0] {
            let issues = check(&sharp.blur(sigma));
            assert!(
                issues.contains(&PageIssue::Blurry) || issues.contains(&PageIssue::LowContrast),
                "sigma {sigma}: {issues:?}"
            );
        }
        let dir = tempfile::tempdir().unwrap();
        let tiny = store_photo(dir.path(), &page_png(400, 500, 235), ImageFormat::Png).unwrap();
        assert!(tiny.page.issues.contains(&PageIssue::LowResolution));
        assert!(PageIssue::LowResolution.blocks());
    }

    #[test]
    fn probe_draws_its_number() {
        let (png, number) = vision_probe().unwrap();
        assert_eq!(number.len(), 3);
        assert!(image::load_from_memory(&png).is_ok());
    }
}
