//! Sticker art for the challenges: original SVGs in the app's engraving
//! style, on jade with the crescent moon (`assets/stickers`). Each has its own
//! outline (round, arch, square, seal) so it is known at a glance even small.
//! A sticker not won yet is the same art in grey, made here from the colour
//! file so the two never drift apart.

use egui::{Image, Vec2};
use kg_engine::ChallengeId;

fn svg(id: ChallengeId) -> &'static str {
    match id {
        ChallengeId::FirstStep => include_str!("../assets/stickers/first_step.svg"),
        ChallengeId::Comeback => include_str!("../assets/stickers/comeback.svg"),
        ChallengeId::PlanInMotion => include_str!("../assets/stickers/plan_in_motion.svg"),
        ChallengeId::WeekOnTrack => include_str!("../assets/stickers/week_on_track.svg"),
    }
}

fn uri(id: ChallengeId, won: bool) -> String {
    let state = if won { "won" } else { "locked" };
    format!("bytes://sticker-{}-{state}.svg", id.key())
}

/// Makes both versions of every sticker loadable by URI, once per context.
fn register(ctx: &egui::Context) {
    let flag = egui::Id::new("stickers_registered");
    if ctx.data(|d| d.get_temp::<bool>(flag)).unwrap_or(false) {
        return;
    }
    for id in ChallengeId::ALL {
        ctx.include_bytes(uri(id, true), svg(id).as_bytes().to_vec());
        ctx.include_bytes(uri(id, false), greyed(svg(id)).into_bytes());
    }
    ctx.data_mut(|d| d.insert_temp(flag, true));
}

/// The sticker at `size` points: in colour when won, grey otherwise.
pub fn image(ctx: &egui::Context, id: ChallengeId, size: f32, won: bool) -> Image<'static> {
    register(ctx);
    let image = Image::new(uri(id, won)).fit_to_exact_size(Vec2::splat(size));
    if won {
        image
    } else {
        image.tint(egui::Color32::from_white_alpha(170))
    }
}

/// The same SVG with every `#rrggbb` colour turned into a light grey of
/// similar lightness: a faded, colourless sticker.
fn greyed(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(at) = rest.find('#') {
        out.push_str(&rest[..at]);
        let hex = rest.get(at + 1..at + 7).filter(|h| {
            h.chars().all(|c| c.is_ascii_hexdigit())
                && !rest[at + 7..].starts_with(|c: char| c.is_ascii_alphanumeric())
        });
        match hex.and_then(grey_of) {
            Some(grey) => {
                out.push_str(&grey);
                rest = &rest[at + 7..];
            }
            None => {
                out.push('#');
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn grey_of(hex: &str) -> Option<String> {
    let channel = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    let (r, g, b) = (channel(0)?, channel(2)?, channel(4)?);
    let lightness = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
    // Pulled toward a light grey: less contrast, so it reads as "not yet".
    let grey = (lightness * 0.55 + 190.0 * 0.45).round().clamp(0.0, 255.0) as u8;
    Some(format!("#{grey:02X}{grey:02X}{grey:02X}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draws(svg: &str) -> resvg::tiny_skia::Pixmap {
        let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &Default::default()).unwrap();
        let mut pixmap = resvg::tiny_skia::Pixmap::new(64, 64).unwrap();
        let scale = 64.0 / tree.size().width();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        pixmap
    }

    #[test]
    fn every_sticker_draws_in_colour_and_in_grey() {
        for id in ChallengeId::ALL {
            let colour = draws(svg(id));
            assert!(
                colour.pixels().iter().any(|p| p.alpha() > 0),
                "{} is empty",
                id.key()
            );
            let grey = draws(&greyed(svg(id)));
            for pixel in grey.pixels() {
                let c = pixel.demultiply();
                assert!(
                    c.red().abs_diff(c.green()) <= 2 && c.green().abs_diff(c.blue()) <= 2,
                    "{} keeps colour when locked",
                    id.key()
                );
            }
        }
    }

    #[test]
    fn greying_changes_only_colours() {
        let svg = r##"<rect fill="#2F6F68" id="a1"/><use href="#a1"/>"##;
        assert_eq!(
            greyed(svg),
            r##"<rect fill="#888888" id="a1"/><use href="#a1"/>"##
        );
    }
}
