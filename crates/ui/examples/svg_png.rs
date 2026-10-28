//! Renders an SVG file to PNG, for checking artwork.
//! cargo run -p kg-ui --example svg_png -- in.svg out.png [size]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use resvg::{tiny_skia, usvg};
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or("missing input")?;
    let output = args.next().ok_or("missing output")?;
    let size: u32 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(256);
    let tree = usvg::Tree::from_data(&std::fs::read(input)?, &usvg::Options::default())?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size).ok_or("bad size")?;
    let scale = size as f32 / tree.size().width().max(tree.size().height());
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.save_png(output)?;
    Ok(())
}
