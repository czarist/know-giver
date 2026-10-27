//! Settings: language, AI server, voice and local models.

use egui::RichText;
use kg_engine::{Command, Engine, ModelId, ModelStatus, PiperVoice, Settings, WhisperModel};
use rust_i18n::t;

use super::Actions;
use crate::app::KnowGiverApp;
use crate::{i18n, theme, widgets};

const FIELD_WIDTH: f32 = 380.0;

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    let draft = app
        .ui
        .settings_draft
        .get_or_insert_with(|| engine.settings().clone());
    widgets::header(ui, &t!("settings.title"), &t!("settings.subtitle"));

    egui::ScrollArea::vertical().show(ui, |ui| {
        language(ui, engine, draft, &mut actions);
        ui.add_space(12.0);
        ai_section(ui, engine, draft, &mut actions);
        ui.add_space(12.0);
        voice_section(ui, draft);
        ui.add_space(12.0);
        models_section(ui, engine, draft, &mut actions);
        ui.add_space(12.0);

        let changed = draft != engine.settings();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(changed, egui::Button::new(t!("settings.save")))
                .clicked()
            {
                actions.send(Command::SaveSettings(draft.clone()));
            }
            if ui
                .add_enabled(changed, egui::Button::new(t!("settings.discard")))
                .clicked()
            {
                *draft = engine.settings().clone();
            }
        });
        ui.add_space(8.0);
        widgets::hint(
            ui,
            &t!(
                "settings.data_dir",
                path = engine.paths().data_dir.display()
            ),
        );
    });
    actions.run(app);
}

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    widgets::card(ui, |ui| {
        ui.label(RichText::new(title).strong().size(17.0));
        ui.add_space(4.0);
        add(ui);
    });
}

fn language(ui: &mut egui::Ui, engine: &Engine, draft: &mut Settings, actions: &mut Actions) {
    section(ui, &t!("settings.language"), |ui| {
        let current = i18n::current();
        egui::ComboBox::from_id_salt("language")
            .selected_text(i18n::language_name(&current))
            .show_ui(ui, |ui| {
                for locale in i18n::LOCALES {
                    if ui
                        .selectable_label(current == locale, i18n::language_name(locale))
                        .clicked()
                        && current != locale
                    {
                        i18n::set(locale);
                        draft.locale = locale.to_owned();
                        // The language applies right away, without the Save button.
                        let mut saved = engine.settings().clone();
                        saved.locale = locale.to_owned();
                        actions.send(Command::SaveSettings(saved));
                    }
                }
            });
    });
}

fn ai_section(ui: &mut egui::Ui, engine: &Engine, draft: &mut Settings, actions: &mut Actions) {
    section(ui, &t!("settings.ai.title"), |ui| {
        widgets::hint(ui, &t!("settings.ai.hint"));
        egui::Grid::new("ai_settings")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .show(ui, |ui| {
                let ai = &mut draft.ai;
                ui.label(t!("settings.ai.base_url"));
                ui.add(egui::TextEdit::singleline(&mut ai.base_url).desired_width(FIELD_WIDTH));
                ui.end_row();

                ui.label(t!("settings.ai.api_key"));
                ui.vertical(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut ai.api_key)
                            .password(true)
                            .desired_width(FIELD_WIDTH),
                    );
                    if engine.api_key_from_env() && ai.api_key == engine.settings().ai.api_key {
                        ui.label(RichText::new(t!("settings.ai.key_from_env")).small());
                    }
                });
                ui.end_row();

                let models = &mut ai.models;
                for (label, value) in [
                    (t!("settings.ai.model_plan"), &mut models.study_plan),
                    (t!("settings.ai.model_tutor"), &mut models.tutor),
                    (t!("settings.ai.model_quiz"), &mut models.quiz),
                    (t!("settings.ai.model_correction"), &mut models.correction),
                    (t!("settings.ai.model_fallback"), &mut models.fallback),
                ] {
                    ui.label(label);
                    ui.add(egui::TextEdit::singleline(value).desired_width(FIELD_WIDTH));
                    ui.end_row();
                }
            });
        ui.checkbox(&mut draft.ai.fast_responses, t!("settings.ai.fast"));
        ui.horizontal(|ui| {
            let testing = engine.is_testing_connection();
            if ui
                .add_enabled(!testing, egui::Button::new(t!("settings.ai.test")))
                .on_hover_text(t!("settings.ai.test_hint"))
                .clicked()
            {
                actions.send(Command::TestConnection);
            }
            if testing {
                ui.spinner();
            }
        });
    });
}

fn voice_section(ui: &mut egui::Ui, draft: &mut Settings) {
    section(ui, &t!("settings.voice.title"), |ui| {
        ui.checkbox(&mut draft.voice_enabled, t!("settings.voice.enabled"));
        ui.checkbox(&mut draft.hands_free, t!("settings.voice.hands_free"));
        ui.checkbox(&mut draft.auto_stop, t!("settings.voice.auto_stop"));
        egui::Grid::new("voice_settings")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .show(ui, |ui| {
                ui.label(t!("settings.voice.voice"));
                egui::ComboBox::from_id_salt("voice")
                    .selected_text(i18n::voice(draft.voice))
                    .show_ui(ui, |ui| {
                        for voice in PiperVoice::ALL {
                            ui.selectable_value(&mut draft.voice, voice, i18n::voice(voice));
                        }
                    });
                ui.end_row();

                ui.label(t!("settings.voice.rate"));
                ui.add(
                    egui::Slider::new(&mut draft.speech_rate, 0.7..=1.5)
                        .step_by(0.05)
                        .fixed_decimals(2),
                );
                ui.end_row();

                ui.label(t!("settings.voice.stt_model"));
                egui::ComboBox::from_id_salt("stt_model")
                    .selected_text(i18n::whisper_model(draft.whisper_model))
                    .show_ui(ui, |ui| {
                        for model in WhisperModel::ALL {
                            ui.selectable_value(
                                &mut draft.whisper_model,
                                model,
                                i18n::whisper_model(model),
                            );
                        }
                    });
                ui.end_row();

                ui.label(t!("settings.quiz_size"));
                ui.add(egui::Slider::new(&mut draft.quiz_size, 3..=10));
                ui.end_row();
            });
    });
}

fn models_section(ui: &mut egui::Ui, engine: &Engine, draft: &Settings, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    section(ui, &t!("settings.models.title"), |ui| {
        widgets::hint(ui, &t!("settings.models.hint"));
        for id in ModelId::all() {
            let in_use = match id {
                ModelId::Whisper(m) => m == draft.whisper_model,
                ModelId::Voice(v) => v == draft.voice,
                ModelId::PiperRuntime => true,
            };
            ui.separator();
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(i18n::model_name(id)).strong());
                        if in_use {
                            widgets::chip(ui, &t!("settings.models.in_use"), p.accent);
                        }
                    });
                    ui.label(
                        RichText::new(t!(
                            "settings.models.size",
                            description = i18n::model_description(id),
                            mb = widgets::megabytes(id.approx_bytes())
                        ))
                        .small()
                        .color(p.muted),
                    );
                });
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| match engine.model_status(id) {
                        ModelStatus::Missing => {
                            if ui.button(t!("settings.models.download")).clicked() {
                                actions.send(Command::DownloadModel(id));
                            }
                            ui.label(RichText::new(t!("settings.models.missing")).color(p.muted));
                        }
                        ModelStatus::Downloading(fraction) => {
                            if ui.button(t!("common.cancel")).clicked() {
                                actions.send(Command::CancelDownload(id));
                            }
                            ui.add(
                                egui::ProgressBar::new(fraction)
                                    .show_percentage()
                                    .desired_width(160.0),
                            );
                        }
                        ModelStatus::Installed { bytes } => {
                            if ui.button(t!("settings.models.remove")).clicked() {
                                actions.send(Command::RemoveModel(id));
                            }
                            ui.label(
                                RichText::new(t!(
                                    "settings.models.installed",
                                    mb = widgets::megabytes(bytes)
                                ))
                                .color(p.success),
                            );
                        }
                    },
                );
            });
        }
        if cfg!(windows) {
            ui.add_space(4.0);
            ui.label(
                RichText::new(t!("settings.models.piper_note"))
                    .small()
                    .color(p.muted),
            );
        }
    });
}
