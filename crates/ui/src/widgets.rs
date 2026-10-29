//! Small shared pieces of UI.

use egui::{Color32, RichText, Sense, Stroke, Vec2};

use crate::theme;

/// Screen title over a heavy rule, and a one-line explanation.
pub fn header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.heading(title);
    rule(ui, 3.0);
    ui.label(RichText::new(subtitle).color(theme::palette(ui.ctx()).muted));
    ui.add_space(12.0);
}

/// A full-width printed rule.
pub fn rule(ui: &mut egui::Ui, thickness: f32) {
    let p = theme::palette(ui.ctx());
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), thickness), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p.ink);
}

/// Small uppercase monospaced label above a title, like a book series name.
pub fn kicker(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.label(
        RichText::new(text.to_uppercase())
            .family(theme::mono_family())
            .size(11.5)
            .color(color),
    );
}

/// A square block with a black frame and a hard shadow.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = theme::palette(ui.ctx());
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.2, p.ink.gamma_multiply(0.85)))
        .shadow(egui::epaint::Shadow {
            offset: [4, 4],
            blur: 0,
            spread: 0,
            color: p.shadow.gamma_multiply(0.9),
        })
        .inner_margin(egui::Margin::same(16))
        .outer_margin(egui::Margin {
            left: 0,
            right: 4,
            top: 0,
            bottom: 4,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A book-cover block: series label, a large title, a line of text, and the
/// Know Giver mark on the right. `add` goes under the text.
pub fn cover(
    ui: &mut egui::Ui,
    series: &str,
    title: &str,
    subtitle: &str,
    art: crate::art::Art,
    art_height: f32,
    add: impl FnOnce(&mut egui::Ui),
) {
    let p = theme::palette(ui.ctx());
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(2.0, p.ink))
        .shadow(egui::epaint::Shadow {
            offset: [6, 6],
            blur: 0,
            spread: 0,
            color: p.shadow,
        })
        .outer_margin(egui::Margin {
            left: 0,
            right: 6,
            top: 0,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Colored band across the top, as on the series' covers.
            let (band, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 8.0), Sense::hover());
            ui.painter().rect_filled(band, 0.0, p.accent);
            ui.horizontal(|ui| {
                let art_width = art_height * art.aspect();
                let text_width = (ui.available_width() - art_width - 40.0).max(160.0);
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.set_width(text_width);
                    ui.add_space(18.0);
                    kicker(ui, series, p.accent);
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(title)
                            .family(theme::heading_family())
                            .size(30.0)
                            .color(p.text),
                    );
                    rule(ui, 3.0);
                    ui.add_space(2.0);
                    ui.label(RichText::new(subtitle).color(p.muted));
                    add(ui);
                    ui.add_space(14.0);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                    // The plate sits on the bottom edge, framed by a black line.
                    let image = ui.add(art.image(art_height));
                    ui.painter().vline(
                        image.rect.left() - 0.5,
                        image.rect.y_range(),
                        Stroke::new(2.0, p.ink),
                    );
                });
            });
        });
}

/// Short colored label, e.g. a difficulty. Painted at its own size so it
/// never stretches to the height of the row around it.
pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    let font = egui::TextStyle::Small.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let padding = Vec2::new(8.0, 3.0);
    let (rect, _) = ui.allocate_exact_size(galley.size() + padding * 2.0, Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, color.gamma_multiply(0.14));
    ui.painter().galley(rect.min + padding, galley, color);
}

/// A friendly empty screen: a large faded icon, a title, one sentence and
/// at most one action. Returns true when the action was clicked.
pub fn empty_state(
    ui: &mut egui::Ui,
    icon: crate::icons::Icon,
    title: &str,
    text: &str,
    action: Option<&str>,
) -> bool {
    let p = theme::palette(ui.ctx());
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        ui.add_space(48.0);
        crate::illustrations::empty(ui, icon);
        ui.add_space(8.0);
        ui.label(
            RichText::new(title)
                .family(theme::heading_family())
                .size(22.0),
        );
        ui.label(RichText::new(text).color(p.muted));
        if let Some(action) = action {
            ui.add_space(8.0);
            clicked = primary_button(ui, action).clicked();
        }
    });
    clicked
}

/// Muted helper text.
pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::palette(ui.ctx()).muted));
}

/// A notice block with a colored left edge.
pub fn callout(ui: &mut egui::Ui, color: Color32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.08))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.4)))
        .corner_radius(0)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// A primary (filled) button.
pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let p = theme::palette(ui.ctx());
    let on_accent = if ui.visuals().dark_mode {
        p.background
    } else {
        Color32::WHITE
    };
    ui.add(
        egui::Button::new(RichText::new(text).color(on_accent).strong())
            .fill(p.accent)
            .stroke(Stroke::new(1.5, p.ink)),
    )
}

/// Bytes as megabytes, e.g. "290".
pub fn megabytes(bytes: u64) -> String {
    format!("{:.0}", bytes as f64 / 1_000_000.0)
}
