//! Small shared pieces of UI.

use egui::{Color32, RichText, Sense, Stroke, Vec2};

use crate::theme;

/// Screen title and one-line explanation.
pub fn header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.heading(title);
    ui.label(RichText::new(subtitle).color(theme::palette(ui.ctx()).muted));
    ui.add_space(12.0);
}

/// A bordered block on the surface color.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = theme::palette(ui.ctx());
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(8)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Short colored label, e.g. a difficulty. Painted at its own size so it
/// never stretches to the height of the row around it.
pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    let font = egui::TextStyle::Small.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let padding = Vec2::new(8.0, 3.0);
    let (rect, _) = ui.allocate_exact_size(galley.size() + padding * 2.0, Sense::hover());
    ui.painter()
        .rect_filled(rect, 10.0, color.gamma_multiply(0.14));
    ui.painter().galley(rect.min + padding, galley, color);
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
        .corner_radius(6)
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
    ui.add(egui::Button::new(RichText::new(text).color(on_accent).strong()).fill(p.accent))
}

/// Bytes as megabytes, e.g. "290".
pub fn megabytes(bytes: u64) -> String {
    format!("{:.0}", bytes as f64 / 1_000_000.0)
}
