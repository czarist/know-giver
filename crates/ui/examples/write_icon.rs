//! Writes the app icon as a Windows `.ico` (uncompressed 32-bit images).
//!
//! cargo run -p kg-ui --example write_icon -- crates/app/assets/know-giver.ico

const SIZES: [u32; 5] = [16, 24, 32, 48, 256];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: write_icon <out.ico>")?;
    let images: Vec<(u32, Vec<u8>)> = SIZES
        .iter()
        .map(|&size| (size, bmp_entry(size, &kg_ui::icon_rgba(size))))
        .collect();

    let mut out = Vec::new();
    // ICONDIR
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, data) in &images {
        // ICONDIRENTRY; 0 means 256.
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &images {
        out.extend_from_slice(data);
    }
    std::fs::write(&path, out)?;
    println!("wrote {path}");
    Ok(())
}

/// BITMAPINFOHEADER + bottom-up BGRA pixels + 1-bit AND mask.
fn bmp_entry(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&40u32.to_le_bytes());
    data.extend_from_slice(&(size as i32).to_le_bytes());
    data.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // XOR + AND
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&32u16.to_le_bytes());
    data.extend_from_slice(&[0; 24]); // no compression, sizes and palette unused
    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            data.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    // AND mask rows are padded to 32 bits; all zero because alpha is used.
    let row_bytes = size.div_ceil(32) * 4;
    data.extend(std::iter::repeat_n(0u8, (row_bytes * size) as usize));
    data
}
