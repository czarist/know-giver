//! The app icon, drawn in code: an open book on a rounded teal square.

/// Icon pixels as RGBA, `size × size`.
pub fn rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            // Sample at the pixel center, in 0..1 units.
            let (u, v) = ((x as f32 + 0.5) / s, (y as f32 + 0.5) / s);
            pixels.extend_from_slice(&pixel(u, v));
        }
    }
    pixels
}

fn pixel(u: f32, v: f32) -> [u8; 4] {
    const TEAL: [u8; 4] = [0x2F, 0x6F, 0x68, 0xFF];
    const PAGE: [u8; 4] = [0xF6, 0xF4, 0xEF, 0xFF];
    const LINE: [u8; 4] = [0xB9, 0xC9, 0xC4, 0xFF];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    if !in_rounded_square(u, v, 0.04, 0.2) {
        return CLEAR;
    }
    // Two pages that rise toward the outer edges, meeting at a spine.
    let spine = 0.5;
    let dx = (u - spine).abs();
    let top = 0.30 + 0.06 * (1.0 - dx / 0.30);
    let bottom = 0.72 + 0.04 * (1.0 - dx / 0.30);
    let on_page = (0.015..0.30).contains(&dx) && v > top && v < bottom;
    if !on_page {
        return TEAL;
    }
    // Text lines on each page.
    let line_phase = ((v - top) / 0.075).fract();
    let in_text = dx > 0.06 && dx < 0.25 && v > top + 0.06 && v < bottom - 0.06;
    if in_text && line_phase < 0.28 {
        LINE
    } else {
        PAGE
    }
}

fn in_rounded_square(u: f32, v: f32, margin: f32, radius: f32) -> bool {
    let (lo, hi) = (margin, 1.0 - margin);
    if u < lo || u > hi || v < lo || v > hi {
        return false;
    }
    let cx = u.clamp(lo + radius, hi - radius);
    let cy = v.clamp(lo + radius, hi - radius);
    (u - cx).powi(2) + (v - cy).powi(2) <= radius * radius
}

pub fn app_icon() -> egui::IconData {
    let size = 128;
    egui::IconData {
        rgba: rgba(size),
        width: size,
        height: size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_are_transparent_and_center_is_opaque() {
        let size = 64;
        let px = rgba(size);
        assert_eq!(px.len(), (size * size * 4) as usize);
        assert_eq!(px[3], 0);
        let center = ((size / 2) * size + size / 2) as usize * 4;
        assert_eq!(px[center + 3], 0xFF);
    }
}
