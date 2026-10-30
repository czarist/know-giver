//! The update offer: a slim bar at the top with a green download button,
//! shown only when a newer version exists.

use egui::{Align, Layout, RichText};
use rust_i18n::t;
use kg_engine::{Command, UpdateStatus};

use crate::app::KnowGiverApp;
use crate::{icons, theme, widgets};

/// The download button's green, dark enough for white text in both themes.
const UPDATE_GREEN: egui::Color32 = egui::Color32::from_rgb(0x23, 0x86, 0x36);

/// Height of the bar, in points.
const HEIGHT: f32 = 34.0;

pub(crate) fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let Some(status) = app.engine.as_ref().ok().and_then(|e| e.update()) else {
        return;
    };
    let p = theme::palette(ui.ctx());
    let frame = egui::Frame::new()
        .fill(p.sidebar)
        .inner_margin(egui::Margin::symmetric(14, 4));
    let mut command = None;
    egui::Panel::top("update")
        .exact_size(HEIGHT)
        .frame(frame)
        .show(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let release = status.release();
                let version = release.version.as_str();
                match &status {
                    UpdateStatus::Available(release) | UpdateStatus::Failed(release) => {
                        let close = egui::Button::image(icons::Icon::Close.image(12.0, p.muted))
                            .frame(false);
                        if ui.add(close).on_hover_text(t!("update.later")).clicked() {
                            command = Some(Command::DismissUpdate);
                        }
                        if release.installable() {
                            if download_button(ui, &t!("update.install")).clicked() {
                                command = Some(Command::InstallUpdate);
                            }
                        } else if download_button(ui, &t!("update.download")).clicked() {
                            crate::app::open_url(&release.page);
                        }
                        widgets::link(ui, &t!("update.notes"), &release.page);
                        let text = if matches!(status, UpdateStatus::Failed(_)) {
                            t!("update.failed", version = version)
                        } else {
                            t!("update.available", version = version)
                        };
                        ui.label(RichText::new(text).small().color(p.muted));
                    }
                    UpdateStatus::Downloading { done, total, .. } => {
                        let fraction = if *total > 0 {
                            (*done as f32 / *total as f32).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        ui.add(
                            egui::ProgressBar::new(fraction)
                                .desired_width(140.0)
                                .fill(UPDATE_GREEN),
                        );
                        ui.label(
                            RichText::new(t!(
                                "update.downloading",
                                version = version,
                                percent = (fraction * 100.0).round() as u32
                            ))
                            .small()
                            .color(p.muted),
                        );
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_millis(200));
                    }
                    UpdateStatus::Installing(_) => {
                        ui.label(
                            RichText::new(t!("update.installing", version = version))
                                .small()
                                .color(p.muted),
                        );
                    }
                }
            });
        });
    if let Some(command) = command {
        app.dispatch(command);
    }
}

/// The green button with the download arrow.
fn download_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let label = RichText::new(text)
        .family(theme::strong_family())
        .color(egui::Color32::WHITE);
    let button = egui::Button::image_and_text(
        icons::Icon::Update.image(14.0, egui::Color32::WHITE),
        label,
    )
    .fill(UPDATE_GREEN)
    .corner_radius(4.0)
    .min_size(egui::vec2(0.0, 24.0));
    ui.add(button)
}
