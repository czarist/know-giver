//! Settings: language, AI server, voice and local models.

use egui::RichText;
use kg_engine::{
    AvatarStyle, Command, Engine, ModelId, ModelStatus, Settings, SttEngine, TtsBackend, Voice,
    VoiceGender, WhisperModel,
};
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
        section(ui, &t!("settings.you.title"), |ui| {
            name_field(ui, &mut draft.student_name);
        });
        ui.add_space(12.0);
        language(ui, engine, draft, &mut actions);
        ui.add_space(12.0);
        ai_section(ui, engine, draft, &mut actions);
        ui.add_space(12.0);
        voice_section(ui, engine, draft, &mut actions);
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

/// "What should we call you?", shared with the welcome screen. Returns the
/// text field's response.
pub(crate) fn name_field(ui: &mut egui::Ui, name: &mut String) -> egui::Response {
    ui.label(t!("settings.you.name"));
    let response = ui.add(
        egui::TextEdit::singleline(name)
            .hint_text(t!("settings.you.name_placeholder"))
            .char_limit(40)
            .desired_width(280.0),
    );
    widgets::hint(ui, &t!("settings.you.name_hint"));
    response
}

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    widgets::card(ui, |ui| {
        ui.label(
            RichText::new(title.to_uppercase())
                .family(theme::heading_family())
                .size(15.0),
        );
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
                    (t!("settings.ai.model_handout"), &mut models.handout),
                    (t!("settings.ai.model_verifier"), &mut models.verifier),
                    (t!("settings.ai.model_vision"), &mut models.vision),
                    (t!("settings.ai.model_fallback"), &mut models.fallback),
                ] {
                    ui.label(label);
                    ui.add(egui::TextEdit::singleline(value).desired_width(FIELD_WIDTH));
                    ui.end_row();
                }
            });
        widgets::hint(ui, &t!("settings.ai.vision_notice"));
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

pub(crate) fn voice_section(
    ui: &mut egui::Ui,
    engine: &Engine,
    draft: &mut Settings,
    actions: &mut Actions,
) {
    section(ui, &t!("settings.voice.title"), |ui| {
        ui.checkbox(&mut draft.voice_enabled, t!("settings.voice.enabled"));
        ui.checkbox(&mut draft.hands_free, t!("settings.voice.hands_free"));
        ui.checkbox(&mut draft.auto_stop, t!("settings.voice.auto_stop"));
        ui.checkbox(&mut draft.barge_in, t!("settings.voice.barge_in"))
            .on_hover_text(t!("settings.voice.barge_in_hint"));
        ui.checkbox(&mut draft.fillers, t!("settings.voice.fillers"));
        ui.horizontal(|ui| {
            ui.label(t!("settings.voice.backend"));
            ui.selectable_value(
                &mut draft.tts_backend,
                TtsBackend::Local,
                t!("settings.voice.local"),
            );
            ui.selectable_value(
                &mut draft.tts_backend,
                TtsBackend::Cloud,
                t!("settings.voice.cloud"),
            );
        });
        match draft.tts_backend {
            TtsBackend::Local => local_voice(ui, engine, draft),
            TtsBackend::Cloud => cloud_voice(ui, draft),
        }
        egui::Grid::new("voice_settings")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .show(ui, |ui| {
                ui.label(t!("settings.voice.rate"));
                ui.add(
                    egui::Slider::new(&mut draft.speech_rate, 0.7..=1.5)
                        .step_by(0.05)
                        .fixed_decimals(2),
                );
                ui.end_row();

                ui.label(t!("settings.voice.pitch"));
                ui.add(egui::Slider::new(&mut draft.speech_pitch, -6.0..=6.0).step_by(0.5));
                ui.end_row();

                ui.label(t!("settings.voice.expressiveness"))
                    .on_hover_text(t!("settings.voice.piper_only"));
                ui.add(
                    egui::Slider::new(
                        &mut draft.speech_expressiveness,
                        kg_engine::EXPRESSIVENESS_RANGE,
                    )
                    .step_by(0.05)
                    .fixed_decimals(2),
                );
                ui.end_row();

                ui.label(t!("settings.voice.rhythm"))
                    .on_hover_text(t!("settings.voice.piper_only"));
                ui.add(
                    egui::Slider::new(
                        &mut draft.speech_rhythm_variation,
                        kg_engine::RHYTHM_VARIATION_RANGE,
                    )
                    .step_by(0.05)
                    .fixed_decimals(2),
                );
                ui.end_row();

                ui.label(t!("settings.voice.avatar"));
                egui::ComboBox::from_id_salt("avatar_style")
                    .selected_text(avatar_style(draft.avatar_style))
                    .show_ui(ui, |ui| {
                        for style in [AvatarStyle::Classic, AvatarStyle::Minimal] {
                            ui.selectable_value(
                                &mut draft.avatar_style,
                                style,
                                avatar_style(style),
                            );
                        }
                    });
                ui.end_row();

                ui.label(t!("settings.voice.stt_model"));
                ui.vertical(|ui| {
                    let selected = match draft.stt_engine {
                        SttEngine::Parakeet => i18n::model_name(ModelId::Parakeet),
                        SttEngine::Whisper => i18n::whisper_model(draft.whisper_model),
                    };
                    egui::ComboBox::from_id_salt("stt_model")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    draft.stt_engine == SttEngine::Parakeet,
                                    i18n::model_name(ModelId::Parakeet),
                                )
                                .clicked()
                            {
                                draft.stt_engine = SttEngine::Parakeet;
                            }
                            for model in WhisperModel::ALL {
                                let chosen = draft.stt_engine == SttEngine::Whisper
                                    && draft.whisper_model == model;
                                if ui
                                    .selectable_label(chosen, i18n::whisper_model(model))
                                    .clicked()
                                {
                                    draft.stt_engine = SttEngine::Whisper;
                                    draft.whisper_model = model;
                                }
                            }
                        });
                    if draft.stt_engine == SttEngine::Parakeet {
                        widgets::hint(ui, &t!("settings.voice.parakeet_hint"));
                    }
                });
                ui.end_row();

                ui.label(t!("settings.quiz_size"));
                ui.add(egui::Slider::new(&mut draft.quiz_size, 3..=10));
                ui.end_row();
            });
        ui.horizontal(|ui| {
            if engine.voice_preview_active() {
                ui.spinner();
                ui.label(t!("settings.voice.preview_active"));
                if ui.button(t!("settings.voice.stop_preview")).clicked() {
                    actions.send(Command::StopVoicePreview);
                }
            } else {
                let available = match draft.tts_backend {
                    TtsBackend::Local => engine.voice_available(draft.voice),
                    TtsBackend::Cloud => {
                        !draft.cloud_speech.base_url.trim().is_empty()
                            && !draft.cloud_speech.model.trim().is_empty()
                            && !draft.cloud_speech.voice.trim().is_empty()
                    }
                };
                if ui
                    .add_enabled(available, egui::Button::new(t!("settings.voice.preview")))
                    .clicked()
                {
                    actions.send(Command::PreviewVoice {
                        settings: draft.clone(),
                        text: t!("settings.voice.sample").into_owned(),
                    });
                }
            }
            widgets::hint(ui, &t!("settings.voice.preview_hint"));
        });
    });
}

fn avatar_style(style: AvatarStyle) -> String {
    match style {
        AvatarStyle::Classic => t!("settings.voice.avatar_classic"),
        AvatarStyle::Minimal => t!("settings.voice.avatar_minimal"),
    }
    .into_owned()
}

fn local_voice(ui: &mut egui::Ui, engine: &Engine, draft: &mut Settings) {
    let filter_id = ui.id().with("voice_gender_filter");
    let mut gender = ui
        .ctx()
        .data_mut(|data| data.get_temp::<VoiceGender>(filter_id))
        .unwrap_or(draft.voice.gender());
    let before = gender;
    ui.horizontal(|ui| {
        ui.label(t!("settings.voice.gender"));
        for option in [VoiceGender::Female, VoiceGender::Male] {
            ui.selectable_value(&mut gender, option, i18n::voice_gender(option));
        }
    });
    ui.ctx()
        .data_mut(|data| data.insert_temp(filter_id, gender));
    let installed: Vec<_> = Voice::ALL
        .into_iter()
        .filter(|voice| voice.gender() == gender && engine.voice_available(*voice))
        .collect();
    if before != gender
        && let Some(voice) = installed.first()
    {
        draft.voice = *voice;
    }
    if installed.is_empty() {
        widgets::hint(ui, &t!("settings.voice.none_installed"));
        let voice = Voice::ALL
            .into_iter()
            .find(|voice| voice.gender() == gender);
        if let Some(voice) = voice {
            ui.label(
                RichText::new(t!(
                    "settings.voice.download_hint",
                    runtime = i18n::model_name(voice.runtime_id()),
                    model = i18n::model_name(voice.model_id())
                ))
                .small(),
            );
        }
    } else {
        let selected = if installed.contains(&draft.voice) {
            i18n::voice(draft.voice)
        } else {
            t!("settings.voice.choose").into_owned()
        };
        ui.horizontal(|ui| {
            ui.label(t!("settings.voice.voice"));
            egui::ComboBox::from_id_salt("installed_voice")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for voice in installed {
                        ui.selectable_value(&mut draft.voice, voice, i18n::voice(voice));
                    }
                });
        });
    }
    ui.label(
        RichText::new(t!(
            "settings.voice.current",
            voice = i18n::voice(draft.voice)
        ))
        .small(),
    );
}

fn cloud_voice(ui: &mut egui::Ui, draft: &mut Settings) {
    widgets::callout(ui, theme::palette(ui.ctx()).warning, |ui| {
        ui.label(t!("settings.voice.cloud_notice"));
    });
    egui::Grid::new("cloud_voice_settings")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            let cloud = &mut draft.cloud_speech;
            for (label, value) in [
                (t!("settings.voice.cloud_url"), &mut cloud.base_url),
                (t!("settings.voice.cloud_model"), &mut cloud.model),
                (t!("settings.voice.cloud_voice"), &mut cloud.voice),
            ] {
                ui.label(label);
                ui.add(egui::TextEdit::singleline(value).desired_width(FIELD_WIDTH));
                ui.end_row();
            }
            ui.label(t!("settings.ai.api_key"));
            ui.add(
                egui::TextEdit::singleline(&mut cloud.api_key)
                    .password(true)
                    .desired_width(FIELD_WIDTH),
            );
            ui.end_row();
            ui.label(t!("settings.voice.gender"));
            ui.horizontal(|ui| {
                for gender in [VoiceGender::Female, VoiceGender::Male] {
                    ui.selectable_value(&mut cloud.gender, gender, i18n::voice_gender(gender));
                }
            });
            ui.end_row();
        });
}

fn models_section(ui: &mut egui::Ui, engine: &Engine, draft: &Settings, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    section(ui, &t!("settings.models.title"), |ui| {
        widgets::hint(ui, &t!("settings.models.hint"));
        for id in ModelId::all() {
            let parakeet = draft.stt_engine == SttEngine::Parakeet;
            let in_use = match id {
                ModelId::Whisper(m) => m == draft.whisper_model,
                ModelId::Parakeet => parakeet,
                ModelId::KokoroRuntime if parakeet => true,
                _ => {
                    draft.tts_backend == TtsBackend::Local
                        && (id == draft.voice.model_id() || id == draft.voice.runtime_id())
                }
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
