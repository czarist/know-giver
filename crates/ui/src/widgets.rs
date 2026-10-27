//! Small shared pieces of UI.

use std::f32::consts::TAU;

use egui::{Color32, RichText, Sense, Stroke, Vec2};
use kg_engine::LessonPhase;

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

/// The round microphone button of the lesson screen. It shows what the
/// lesson is doing: still when idle, follows the voice while listening,
/// rotates while thinking and ripples while speaking.
pub fn voice_orb(
    ui: &mut egui::Ui,
    phase: LessonPhase,
    level: f32,
    enabled: bool,
) -> egui::Response {
    let size = Vec2::splat(96.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let p = theme::palette(ui.ctx());
    let painter = ui.painter_at(rect.expand(4.0));
    let center = rect.center();
    let time = ui.input(|i| i.time) as f32;
    let base = 30.0;

    let accent = if enabled { p.accent } else { p.muted };
    match phase {
        LessonPhase::Idle => {
            let hover = if response.hovered() { 2.0 } else { 0.0 };
            painter.circle_filled(center, base + hover, accent.gamma_multiply(0.15));
            painter.circle_stroke(center, base + hover, Stroke::new(1.5, accent));
        }
        LessonPhase::Listening => {
            let r = base + 14.0 * level.clamp(0.0, 1.0);
            painter.circle_filled(center, r + 6.0, accent.gamma_multiply(0.10));
            painter.circle_filled(center, r, accent.gamma_multiply(0.35));
            painter.circle_stroke(center, base, Stroke::new(2.0, accent));
        }
        LessonPhase::Transcribing | LessonPhase::Thinking => {
            painter.circle_filled(center, base, accent.gamma_multiply(0.12));
            for i in 0..3 {
                let angle = time * 2.2 + i as f32 * TAU / 3.0;
                let dot = center + Vec2::angled(angle) * (base - 6.0);
                painter.circle_filled(dot, 4.0, accent);
            }
        }
        LessonPhase::Speaking => {
            for i in 0..3 {
                let t = (time * 0.8 + i as f32 / 3.0).fract();
                let alpha = (1.0 - t) * 0.5;
                painter.circle_stroke(
                    center,
                    base + t * 16.0,
                    Stroke::new(1.5, accent.gamma_multiply(alpha)),
                );
            }
            painter.circle_filled(center, base, accent.gamma_multiply(0.3));
        }
    }
    mic_glyph(&painter, center, accent);
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// A simple microphone drawn with shapes, so no icon font is needed.
fn mic_glyph(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let stroke = Stroke::new(2.0, color);
    let capsule = egui::Rect::from_center_size(center - Vec2::new(0.0, 4.0), Vec2::new(10.0, 18.0));
    painter.rect_stroke(capsule, 5.0, stroke, egui::StrokeKind::Middle);
    let arc: Vec<egui::Pos2> = (0..=16)
        .map(|i| {
            let a = std::f32::consts::PI * i as f32 / 16.0;
            center + Vec2::new(-9.0 * a.cos(), 2.0 + 9.0 * a.sin())
        })
        .collect();
    painter.add(egui::Shape::line(arc, stroke));
    painter.line_segment(
        [center + Vec2::new(0.0, 11.0), center + Vec2::new(0.0, 15.0)],
        stroke,
    );
}

/// Bytes as megabytes, e.g. "290".
pub fn megabytes(bytes: u64) -> String {
    format!("{:.0}", bytes as f64 / 1_000_000.0)
}
