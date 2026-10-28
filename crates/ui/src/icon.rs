//! The app icon, rasterized from the logo SVG.

/// The Know Giver mark (open book in a speech bubble).
pub const LOGO_SVG: &[u8] = include_bytes!("../assets/logo.svg");

/// Icon pixels as straight (not premultiplied) RGBA, `size × size`.
/// Falls back to a plain teal square if the SVG cannot be rendered.
pub fn rgba(size: u32) -> Vec<u8> {
    render(LOGO_SVG, size).unwrap_or_else(|| {
        [0x2F, 0x6F, 0x68, 0xFF]
            .into_iter()
            .cycle()
            .take((size * size * 4) as usize)
            .collect()
    })
}

fn render(svg: &[u8], size: u32) -> Option<Vec<u8>> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let scale = size as f32 / tree.size().width().max(tree.size().height());
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(
        pixmap
            .pixels()
            .iter()
            .flat_map(|p| {
                let c = p.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect(),
    )
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
    fn logo_renders_with_transparent_corners() {
        let size = 64;
        let px = render(LOGO_SVG, size).unwrap();
        assert_eq!(px.len(), (size * size * 4) as usize);
        // Top-left corner is outside the bubble.
        assert_eq!(px[3], 0);
        // The middle of the bubble's top edge is teal and opaque.
        let i = ((8 * size + size / 2) * 4) as usize;
        assert_eq!(&px[i..i + 4], &[0x2F, 0x6F, 0x68, 0xFF]);
    }
}
