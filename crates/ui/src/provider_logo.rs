//! Logos of AI providers (Simple Icons, CC0; see `assets/providers`), or a
//! monogram for providers without one.

use egui::{Color32, Image, ImageSource, Rect, Sense, Vec2, include_image};
use kg_engine::Provider;

use crate::theme;

fn source(id: &str) -> Option<ImageSource<'static>> {
    Some(match id {
        "anthropic" => include_image!("../assets/providers/claude.svg"),
        "gemini" => include_image!("../assets/providers/googlegemini.svg"),
        "deepseek" => include_image!("../assets/providers/deepseek.svg"),
        "mistral" => include_image!("../assets/providers/mistralai.svg"),
        "openrouter" => include_image!("../assets/providers/openrouter.svg"),
        "ollama" => include_image!("../assets/providers/ollama.svg"),
        "perplexity" => include_image!("../assets/providers/perplexity.svg"),
        "nvidia" => include_image!("../assets/providers/nvidia.svg"),
        "qwen" | "bailian" => include_image!("../assets/providers/qwen.svg"),
        "moonshot" | "moonshot-cn" => include_image!("../assets/providers/kimi.svg"),
        "lmstudio" => include_image!("../assets/providers/lmstudio.svg"),
        _ => return None,
    })
}

/// Letters drawn when there is no logo.
fn monogram(provider: &Provider) -> String {
    match provider.id {
        "openai" => "AI".to_owned(),
        "custom" => "API".to_owned(),
        _ => provider
            .name
            .chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default(),
    }
}

/// Paints the provider's logo in `rect`.
pub fn paint(ui: &egui::Ui, rect: Rect, provider: &Provider) {
    let p = theme::palette(ui.ctx());
    if let Some(source) = source(provider.id) {
        Image::new(source)
            .tint(p.text)
            .fit_to_exact_size(rect.size())
            .paint_at(ui, rect);
        return;
    }
    let painter = ui.painter();
    let radius = rect.width().min(rect.height()) / 2.0;
    painter.circle_filled(rect.center(), radius, p.accent.gamma_multiply(0.25));
    painter.circle_stroke(rect.center(), radius, egui::Stroke::new(1.0, p.accent));
    let letters = monogram(provider);
    let size = if letters.chars().count() > 1 {
        radius * 0.8
    } else {
        radius * 1.1
    };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        letters,
        egui::FontId::new(size, theme::strong_family()),
        p.text,
    );
}

/// The logo as a widget of `size` points.
pub fn show(ui: &mut egui::Ui, provider: &Provider, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint(ui, rect, provider);
    response
}

/// A row in the provider list: logo, name and an optional note.
pub fn row(
    ui: &mut egui::Ui,
    provider: &Provider,
    name: &str,
    note: Option<&str>,
    selected: bool,
) -> egui::Response {
    let p = theme::palette(ui.ctx());
    let width = ui.available_width().max(260.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 32.0), Sense::click());
    let fill = if selected {
        p.accent.gamma_multiply(0.18)
    } else if response.hovered() {
        p.accent.gamma_multiply(0.08)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    let logo = Rect::from_center_size(
        egui::pos2(rect.left() + 18.0, rect.center().y),
        Vec2::splat(18.0),
    );
    paint(ui, logo, provider);
    let font = egui::TextStyle::Body.resolve(ui.style());
    ui.painter().text(
        egui::pos2(rect.left() + 38.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        font.clone(),
        if selected {
            p.text
        } else {
            ui.visuals().text_color()
        },
    );
    if let Some(note) = note {
        ui.painter().text(
            egui::pos2(rect.right() - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            note,
            egui::FontId::new(font.size * 0.82, font.family),
            p.highlight,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
