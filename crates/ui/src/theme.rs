//! Calm, low-glare palette and typography for long study sessions.
//!
//! Fonts (SIL Open Font License, files in `assets/fonts`): Literata for
//! headings, designed for long reading; Inter for everything else.

use std::sync::Arc;

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Theme,
    Visuals,
};

/// Colors for one theme.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: Color32,
    pub sidebar: Color32,
    pub surface: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    /// Warm amber for highlights: streaks, milestones, "speaking".
    pub highlight: Color32,
    pub border: Color32,
    pub success: Color32,
    pub danger: Color32,
    pub warning: Color32,
}

const LIGHT: Palette = Palette {
    background: Color32::from_rgb(0xF6, 0xF4, 0xEF),
    sidebar: Color32::from_rgb(0xEC, 0xE9, 0xE2),
    surface: Color32::from_rgb(0xFF, 0xFE, 0xFB),
    text: Color32::from_rgb(0x2B, 0x2A, 0x28),
    muted: Color32::from_rgb(0x6E, 0x6A, 0x63),
    accent: Color32::from_rgb(0x2F, 0x6F, 0x68),
    highlight: Color32::from_rgb(0xB0, 0x77, 0x1F),
    border: Color32::from_rgb(0xD9, 0xD5, 0xCC),
    success: Color32::from_rgb(0x3B, 0x7A, 0x4A),
    danger: Color32::from_rgb(0xA8, 0x4A, 0x3F),
    warning: Color32::from_rgb(0x9A, 0x6B, 0x1F),
};

const DARK: Palette = Palette {
    background: Color32::from_rgb(0x1C, 0x1E, 0x20),
    sidebar: Color32::from_rgb(0x23, 0x26, 0x29),
    surface: Color32::from_rgb(0x2A, 0x2D, 0x31),
    text: Color32::from_rgb(0xE4, 0xE1, 0xDA),
    muted: Color32::from_rgb(0x9C, 0x98, 0x90),
    accent: Color32::from_rgb(0x8C, 0xC2, 0xB8),
    highlight: Color32::from_rgb(0xE2, 0xB1, 0x5E),
    border: Color32::from_rgb(0x3A, 0x3E, 0x43),
    success: Color32::from_rgb(0x8F, 0xC2, 0x9A),
    danger: Color32::from_rgb(0xE0, 0x96, 0x8B),
    warning: Color32::from_rgb(0xD9, 0xB3, 0x6C),
};

/// Font family for headings and the wordmark.
pub fn heading_family() -> FontFamily {
    FontFamily::Name("heading".into())
}

/// Font family for strong UI text (semibold Inter).
pub fn strong_family() -> FontFamily {
    FontFamily::Name("strong".into())
}

/// Applies the Know Giver look for both light and dark mode.
/// egui follows the system theme by default.
pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_visuals_of(Theme::Light, visuals(Visuals::light(), &LIGHT));
    ctx.set_visuals_of(Theme::Dark, visuals(Visuals::dark(), &DARK));

    ctx.all_styles_mut(|style| {
        use FontFamily::{Monospace, Proportional};
        style.text_styles = [
            (TextStyle::Heading, FontId::new(28.0, heading_family())),
            (TextStyle::Body, FontId::new(15.5, Proportional)),
            (TextStyle::Button, FontId::new(14.5, Proportional)),
            (TextStyle::Small, FontId::new(12.5, Proportional)),
            (TextStyle::Monospace, FontId::new(13.5, Monospace)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
        style.spacing.interact_size.y = 30.0;
    });
}

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add("inter", include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    add(
        "inter-semibold",
        include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    );
    add(
        "literata-semibold",
        include_bytes!("../assets/fonts/Literata-SemiBold.ttf"),
    );

    // egui's default fonts stay as fallbacks for symbols the new fonts lack.
    let defaults = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let with_defaults = |first: &str| {
        std::iter::once(first.to_owned())
            .chain(defaults.iter().cloned())
            .collect::<Vec<_>>()
    };
    fonts
        .families
        .insert(FontFamily::Proportional, with_defaults("inter"));
    fonts
        .families
        .insert(heading_family(), with_defaults("literata-semibold"));
    fonts
        .families
        .insert(strong_family(), with_defaults("inter-semibold"));
    fonts
}

fn visuals(mut v: Visuals, p: &Palette) -> Visuals {
    v.panel_fill = p.background;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.sidebar;
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    // Also the fill of progress bars, so it must read well.
    v.selection.bg_fill = p.accent.gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    let radius = CornerRadius::same(7);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = radius;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.weak_bg_fill = p.surface;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v
}

/// Palette for the current theme.
pub fn palette(ctx: &egui::Context) -> Palette {
    match ctx.theme() {
        Theme::Light => LIGHT,
        Theme::Dark => DARK,
    }
}
