//! First run: three short steps (AI, voice, edital) before the first plan.

use egui::{Color32, RichText};
use kg_engine::{Command, Engine, ModelId, ModelStatus, PiperVoice, WhisperModel};
use rust_i18n::t;

use super::Actions;
use crate::app::{KnowGiverApp, Screen};
use crate::icons::{self, Icon};
use crate::{theme, widgets};

/// Models the voice step downloads.
const VOICE_MODELS: [ModelId; 3] = [
    ModelId::Whisper(WhisperModel::Base),
    ModelId::PiperRuntime,
    ModelId::Voice(PiperVoice::Faber),
];

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.set_max_width(600.0);
            ui.add_space(12.0);
            ui.add(icons::logo(76.0));
            ui.label(
                RichText::new(t!("welcome.title"))
                    .family(theme::heading_family())
                    .size(30.0),
            );
            widgets::hint(ui, &t!("welcome.subtitle"));
            ui.add_space(16.0);
        });
        ui.vertical_centered_justified(|ui| {
            ui.set_max_width(600.0);
            ai_step(ui, engine, &mut actions);
            voice_step(ui, engine, &mut actions);
            edital_step(ui, &mut actions);
            ui.add_space(8.0);
            if ui.button(t!("welcome.skip")).clicked() {
                actions.send(Command::FinishOnboarding);
                actions.go_to(Screen::Edital);
            }
        });
    });
    actions.run(app);
}

/// A numbered step with a check mark once done.
fn step(ui: &mut egui::Ui, number: u8, done: bool, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let p = theme::palette(ui.ctx());
    widgets::card(ui, |ui| {
        // Left-aligned inside, even though the column is centered.
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.horizontal(|ui| {
                if done {
                    ui.add(Icon::Check.image(22.0, p.success));
                } else {
                    ui.label(
                        RichText::new(number.to_string())
                            .family(theme::heading_family())
                            .size(20.0)
                            .color(p.accent),
                    );
                }
                ui.label(
                    RichText::new(title)
                        .family(theme::strong_family())
                        .size(16.0),
                );
            });
            add(ui);
        });
    });
    ui.add_space(6.0);
}

fn ai_step(ui: &mut egui::Ui, engine: &Engine, actions: &mut Actions) {
    let configured = !engine.settings().ai.api_key.is_empty()
        || engine.settings().ai.base_url.contains("localhost");
    step(ui, 1, configured, &t!("welcome.ai.title"), |ui| {
        let note = if engine.api_key_from_env() {
            t!("welcome.ai.from_env")
        } else if configured {
            t!("welcome.ai.ready")
        } else {
            t!("welcome.ai.missing")
        };
        widgets::hint(ui, &note);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !engine.is_testing_connection(),
                    egui::Button::new(t!("settings.ai.test")),
                )
                .clicked()
            {
                actions.send(Command::TestConnection);
            }
            if engine.is_testing_connection() {
                ui.spinner();
            }
            if ui.button(t!("lesson.open_settings")).clicked() {
                actions.go_to(Screen::Settings);
            }
        });
    });
}

fn voice_step(ui: &mut egui::Ui, engine: &Engine, actions: &mut Actions) {
    let statuses: Vec<ModelStatus> = VOICE_MODELS
        .iter()
        .map(|id| engine.model_status(*id))
        .collect();
    let installed = statuses
        .iter()
        .filter(|s| matches!(s, ModelStatus::Installed { .. }))
        .count();
    let downloading: Vec<f32> = statuses
        .iter()
        .filter_map(|s| match s {
            ModelStatus::Downloading(f) => Some(*f),
            _ => None,
        })
        .collect();
    let done = installed == VOICE_MODELS.len();
    step(ui, 2, done, &t!("welcome.voice.title"), |ui| {
        widgets::hint(ui, &t!("welcome.voice.text"));
        if done {
            return;
        }
        if downloading.is_empty() {
            let label = t!("welcome.voice.download");
            if widgets::primary_button(ui, &label).clicked() {
                for id in VOICE_MODELS {
                    actions.send(Command::DownloadModel(id));
                }
            }
        } else {
            let total: f32 = statuses
                .iter()
                .map(|s| match s {
                    ModelStatus::Installed { .. } => 1.0,
                    ModelStatus::Downloading(f) => *f,
                    ModelStatus::Missing => 0.0,
                })
                .sum();
            ui.add(
                egui::ProgressBar::new(total / VOICE_MODELS.len() as f32)
                    .show_percentage()
                    .desired_height(8.0),
            );
        }
    });
}

fn edital_step(ui: &mut egui::Ui, actions: &mut Actions) {
    step(ui, 3, false, &t!("welcome.edital.title"), |ui| {
        widgets::hint(ui, &t!("welcome.edital.text"));
        let p = theme::palette(ui.ctx());
        let start = egui::Button::image_and_text(
            Icon::Edital.image(16.0, Color32::WHITE),
            RichText::new(t!("edital.select_pdf")).color(Color32::WHITE),
        )
        .fill(p.accent);
        if ui.add(start).clicked() {
            actions.send(Command::FinishOnboarding);
            actions.go_to(Screen::Edital);
        }
    });
}
