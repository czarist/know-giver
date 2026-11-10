//! Settings: language, AI server, voice and local models.

use egui::RichText;
use kg_engine::{
    Command, Engine, EnglishVoice, ManuscriptOcr, ModelId, ModelStatus, Settings, SttEngine,
    Teacher, TtsBackend, Voice, VoiceGender, WhisperModel,
};
use rust_i18n::t;

use super::Actions;
use crate::app::KnowGiverApp;
use crate::{i18n, teacher, theme, widgets};

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
        section(ui, &t!("settings.teacher.title"), |ui| {
            teacher_picker(ui, engine, draft, false);
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
        section(ui, &t!("settings.study_reminders.title"), |ui| {
            ui.checkbox(
                &mut draft.study_reminders,
                t!("settings.study_reminders.enabled"),
            );
            ui.add_enabled_ui(draft.study_reminders, |ui| {
                ui.horizontal(|ui| {
                    ui.label(t!("settings.study_reminders.interval"));
                    egui::ComboBox::from_id_salt("reminder_interval")
                        .selected_text(crate::reminders::interval_label(draft.reminder_minutes))
                        .show_ui(ui, |ui| {
                            for minutes in crate::reminders::REMINDER_CHOICES {
                                ui.selectable_value(
                                    &mut draft.reminder_minutes,
                                    minutes,
                                    crate::reminders::interval_label(minutes),
                                );
                            }
                        });
                });
            });
            widgets::hint(ui, &t!("settings.study_reminders.hint"));
        });
        ui.add_space(12.0);
        about_section(ui, draft);
        ui.add_space(12.0);
        legal_section(ui, engine, &mut actions);
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
        provider_picker(ui, draft);
        let provider = kg_engine::ai_provider(&draft.ai_provider)
            .or_else(|| kg_engine::provider_for_url(&draft.ai.base_url));
        let account = provider.and_then(kg_engine::Provider::account);
        let is_copilot = account == Some(kg_engine::AccountKind::Copilot);
        if is_copilot {
            draft.manuscript_ocr = ManuscriptOcr::Local;
        }
        if let Some(kind) = account {
            ui.add_space(4.0);
            crate::accounts::panel(ui, engine, kind, actions);
            ui.add_space(8.0);
        }
        let options = model_options(engine, provider, &draft.ai);
        let can_list = match account {
            Some(kind) => crate::accounts::connected(engine, kind),
            None => {
                draft.ai.base_url.trim().starts_with("http")
                    && (!provider.is_some_and(|p| p.needs_key)
                        || !draft.ai.api_key.trim().is_empty())
            }
        };
        let listed = engine.model_list().filter(|list| list.is_for(&draft.ai));
        // A signed-in account offers its own models: list them once.
        if account.is_some() && can_list && listed.is_none() && !engine.is_listing_models() {
            actions.send(Command::ListModels(Box::new(draft.ai.clone())));
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_list && !engine.is_listing_models(),
                    egui::Button::new(t!("setup.ai.load_models")),
                )
                .on_hover_text(t!("setup.ai.load_models_hint"))
                .clicked()
            {
                actions.send(Command::ListModels(Box::new(draft.ai.clone())));
            }
            if engine.is_listing_models() {
                ui.spinner();
            } else if let Some(list) = listed {
                let text = match &list.result {
                    Ok(models) => t!("setup.ai.models_loaded", count = models.len()),
                    Err(_) => t!("setup.ai.models_failed"),
                };
                ui.label(
                    RichText::new(text)
                        .small()
                        .color(theme::palette(ui.ctx()).muted),
                );
            }
        });
        ui.add_space(6.0);
        egui::Grid::new("ai_settings")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .show(ui, |ui| {
                let ai = &mut draft.ai;
                if account.is_none() {
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
                }

                let models = &mut ai.models;
                let default = t!("settings.ai.use_default");
                let none = t!("settings.ai.no_model");
                for (id, label, value, show, empty) in [
                    (
                        "plan",
                        t!("settings.ai.model_plan"),
                        &mut models.study_plan,
                        true,
                        None,
                    ),
                    (
                        "tutor",
                        t!("settings.ai.model_tutor"),
                        &mut models.tutor,
                        true,
                        None,
                    ),
                    (
                        "quiz",
                        t!("settings.ai.model_quiz"),
                        &mut models.quiz,
                        true,
                        None,
                    ),
                    (
                        "correction",
                        t!("settings.ai.model_correction"),
                        &mut models.correction,
                        true,
                        None,
                    ),
                    (
                        "handout",
                        t!("settings.ai.model_handout"),
                        &mut models.handout,
                        true,
                        Some(default.as_ref()),
                    ),
                    (
                        "verifier",
                        t!("settings.ai.model_verifier"),
                        &mut models.verifier,
                        true,
                        Some(default.as_ref()),
                    ),
                    (
                        "vision",
                        t!("settings.ai.model_vision"),
                        &mut models.vision,
                        !is_copilot,
                        Some(none.as_ref()),
                    ),
                    (
                        "fallback",
                        t!("settings.ai.model_fallback"),
                        &mut models.fallback,
                        true,
                        Some(none.as_ref()),
                    ),
                ] {
                    if !show {
                        continue;
                    }
                    ui.label(label);
                    model_field(ui, id, value, &options, provider, empty);
                    ui.end_row();
                }
            });
        if is_copilot {
            widgets::hint(ui, &t!("copilot.no_vision"));
        } else {
            widgets::hint(ui, &t!("settings.ai.vision_notice"));
        }
        ui.separator();
        ui.label(t!("settings.manuscript_ocr.title"));
        ui.add_enabled_ui(!is_copilot, |ui| {
            ui.radio_value(
                &mut draft.manuscript_ocr,
                ManuscriptOcr::Local,
                t!("settings.manuscript_ocr.local"),
            );
            ui.radio_value(
                &mut draft.manuscript_ocr,
                ManuscriptOcr::ExternalVision,
                t!("settings.manuscript_ocr.external"),
            );
        });
        let ocr_hint = if is_copilot {
            t!("settings.manuscript_ocr.copilot")
        } else if draft.manuscript_ocr == ManuscriptOcr::Local {
            t!("settings.manuscript_ocr.local_hint")
        } else {
            t!("settings.manuscript_ocr.external_hint")
        };
        widgets::hint(ui, &ocr_hint);
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
        ui.add_enabled_ui(draft.auto_stop, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.label(t!("settings.voice.turn_pause"));
                ui.add(
                    egui::Slider::new(&mut draft.turn_pause, kg_engine::TURN_PAUSE_RANGE)
                        .step_by(0.1)
                        .fixed_decimals(1)
                        .suffix(" s"),
                )
                .on_hover_text(t!("settings.voice.turn_pause_hint"));
            });
        });
        ui.checkbox(&mut draft.barge_in, t!("settings.voice.barge_in"))
            .on_hover_text(t!("settings.voice.barge_in_hint"));
        ui.checkbox(&mut draft.fillers, t!("settings.voice.fillers"));
        ui.checkbox(&mut draft.board_images, t!("settings.voice.board_images"))
            .on_hover_text(t!("settings.voice.board_images_hint"));
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

                ui.label(t!("settings.lesson_length.title"))
                    .on_hover_text(t!("settings.lesson_length.hint"));
                ui.horizontal(|ui| {
                    for length in kg_engine::LessonLength::ALL {
                        ui.selectable_value(
                            &mut draft.lesson_length,
                            length,
                            crate::i18n::lesson_length(length),
                        )
                        .on_hover_text(crate::i18n::lesson_length_hint(length));
                    }
                });
                ui.end_row();

                english_voice(ui, engine, draft, actions);
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

/// The Kokoro voice for English passages in lessons, with a sample.
fn english_voice(ui: &mut egui::Ui, engine: &Engine, draft: &mut Settings, actions: &mut Actions) {
    ui.label(t!("settings.english_voice.title"))
        .on_hover_text(t!("settings.english_voice.hint"));
    let kokoro = engine.voice_available(Voice::Kokoro(kg_engine::KokoroVoice::default()));
    ui.horizontal(|ui| {
        let name = |voice: Option<EnglishVoice>| match voice {
            Some(voice) => i18n::english_voice(voice),
            None => t!("settings.english_voice.off").into_owned(),
        };
        ui.add_enabled_ui(kokoro, |ui| {
            egui::ComboBox::from_id_salt("english_voice")
                .selected_text(name(draft.english_voice))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut draft.english_voice, None, name(None));
                    for voice in EnglishVoice::ALL {
                        ui.selectable_value(
                            &mut draft.english_voice,
                            Some(voice),
                            name(Some(voice)),
                        );
                    }
                });
        })
        .response
        .on_disabled_hover_text(t!("settings.english_voice.need_kokoro"));
        let can_try = kokoro && draft.english_voice.is_some() && !engine.voice_preview_active();
        if ui
            .add_enabled(can_try, egui::Button::new(t!("settings.english_voice.try")))
            .clicked()
        {
            actions.send(Command::PreviewVoice {
                settings: draft.clone(),
                text: t!("settings.english_voice.sample").into_owned(),
            });
        }
    });
}

/// One card per teacher. Picking one also picks their voice when it is
/// installed, or another installed voice of the same gender.
/// Teacher cards. With `require_choice` (first run) none is marked until
/// the student picks one.
pub(crate) fn teacher_picker(
    ui: &mut egui::Ui,
    engine: &Engine,
    draft: &mut Settings,
    require_choice: bool,
) {
    let p = theme::palette(ui.ctx());
    widgets::hint(ui, &t!("settings.teacher.hint"));
    ui.add_space(6.0);
    let current = if require_choice {
        draft.teacher
    } else {
        Some(draft.teacher())
    };
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
        for option in Teacher::ALL {
            let selected = Some(option) == current;
            let response = egui::Frame::new()
                .fill(if selected {
                    p.accent.gamma_multiply(0.12)
                } else {
                    p.surface
                })
                .stroke(egui::Stroke::new(
                    if selected { 2.5 } else { 1.0 },
                    if selected { p.accent } else { p.border },
                ))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(132.0);
                    ui.vertical_centered(|ui| {
                        ui.add(teacher::still(ui.ctx(), option, 104.0));
                        ui.label(RichText::new(i18n::teacher_name(option)).strong());
                        ui.label(
                            RichText::new(i18n::teacher_about(option))
                                .small()
                                .color(p.muted),
                        );
                    });
                })
                .response
                .interact(egui::Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() {
                picked = Some(option);
            }
        }
    });
    let Some(option) = picked else {
        return;
    };
    draft.teacher = Some(option);
    if draft.tts_backend == TtsBackend::Local {
        let voice = if engine.voice_available(option.voice()) {
            Some(option.voice())
        } else {
            Voice::ALL
                .into_iter()
                .find(|v| v.gender() == option.gender() && engine.voice_available(*v))
        };
        if let Some(voice) = voice {
            draft.voice = voice;
        }
        // Let the voice list follow the teacher's gender.
        ui.ctx()
            .data_mut(|d| d.remove::<VoiceGender>(egui::Id::new("voice_gender_filter")));
    } else {
        draft.cloud_speech.gender = option.gender();
    }
}

fn local_voice(ui: &mut egui::Ui, engine: &Engine, draft: &mut Settings) {
    let filter_id = egui::Id::new("voice_gender_filter");
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
            .find(|voice| voice.gender() == gender && voice.runtime_id().downloadable());
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
                        ModelStatus::Missing if !id.downloadable() => {
                            ui.label(
                                RichText::new(t!("settings.models.unavailable")).color(p.muted),
                            );
                        }
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
        let note = if ModelId::PiperRuntime.downloadable() {
            t!("settings.models.piper_note")
        } else {
            t!("settings.models.piper_unavailable")
        };
        ui.add_space(4.0);
        ui.label(RichText::new(note).small().color(p.muted));
    });
}

/// The service's suggested models, then the ones its account listed.
fn model_options(
    engine: &Engine,
    provider: Option<&kg_engine::Provider>,
    config: &kg_engine::AiConfig,
) -> Vec<String> {
    let mut options: Vec<String> = provider
        .map(|p| p.models.iter().map(|m| m.id.to_owned()).collect())
        .unwrap_or_default();
    let listed = engine
        .model_list()
        .filter(|list| list.is_for(config))
        .and_then(|list| list.result.as_ref().ok());
    for model in listed.into_iter().flatten() {
        if !options.contains(model) {
            options.push(model.clone());
        }
    }
    options
}

/// A menu of the known models; "Other model" opens a field for any name.
/// `empty` names the blank choice of an optional field.
fn model_field(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut String,
    options: &[String],
    provider: Option<&kg_engine::Provider>,
    empty: Option<&str>,
) {
    let typing_id = egui::Id::new(("settings_model_typing", id));
    let mut typing = ui.data(|d| d.get_temp::<bool>(typing_id)).unwrap_or(false)
        || (!value.trim().is_empty() && !options.contains(value));
    let selected = if typing {
        t!("setup.ai.other_model").into_owned()
    } else if value.trim().is_empty() {
        empty.map_or_else(|| t!("setup.ai.choose").into_owned(), str::to_owned)
    } else {
        value.clone()
    };
    ui.vertical(|ui| {
        egui::ComboBox::from_id_salt(("settings_model", id))
            .selected_text(selected)
            .width(FIELD_WIDTH)
            .height(360.0)
            .show_ui(ui, |ui| {
                ui.set_min_width(FIELD_WIDTH);
                if let Some(empty) = empty
                    && ui
                        .selectable_label(!typing && value.trim().is_empty(), empty)
                        .clicked()
                {
                    value.clear();
                    typing = false;
                }
                for option in options {
                    let text = if provider.is_some_and(|p| p.model_sees(option)) {
                        format!("{option}   · {}", t!("setup.ai.reads_images"))
                    } else {
                        option.clone()
                    };
                    if ui
                        .selectable_label(!typing && value == option, text)
                        .clicked()
                    {
                        value.clone_from(option);
                        typing = false;
                    }
                }
                ui.separator();
                if ui
                    .selectable_label(typing, t!("setup.ai.other_model"))
                    .clicked()
                {
                    typing = true;
                }
            });
        if typing {
            ui.add(
                egui::TextEdit::singleline(value)
                    .hint_text(t!("setup.ai.model_placeholder"))
                    .desired_width(FIELD_WIDTH),
            );
        }
    });
    ui.data_mut(|d| d.insert_temp(typing_id, typing));
}

/// Picks an AI service: fills the address and one model for every task.
/// The key typed so far is kept.
fn provider_picker(ui: &mut egui::Ui, draft: &mut Settings) {
    let mainland = draft
        .region
        .is_some_and(kg_engine::Region::is_mainland_china);
    let current = kg_engine::ai_provider(&draft.ai_provider)
        .or_else(|| kg_engine::provider_for_url(&draft.ai.base_url));
    ui.horizontal_wrapped(|ui| {
        ui.label(t!("settings.ai.provider"));
        let selected = current.map_or_else(
            || t!("setup.ai.custom").into_owned(),
            super::setup::provider_name,
        );
        if let Some(provider) = current {
            crate::provider_logo::show(ui, provider, 20.0);
        }
        egui::ComboBox::from_id_salt("settings_provider")
            .selected_text(selected)
            .width(260.0)
            .height(460.0)
            .show_ui(ui, |ui| {
                let clicked = super::setup::provider_list(ui, current.map(|p| p.id), mainland);
                if let Some(provider) = clicked {
                    let picked = current.map(|p| p.id) == Some(provider.id);
                    if !picked {
                        let base_url = if provider.base_url.is_empty() {
                            draft.ai.base_url.clone()
                        } else {
                            provider.base_url.to_owned()
                        };
                        let fast = draft.ai.fast_responses;
                        draft.ai =
                            provider.config(&base_url, &draft.ai.api_key, provider.default_model());
                        draft.ai.fast_responses = fast;
                        draft.ai_provider = provider.id.to_owned();
                    }
                }
            });
        if let Some(provider) = current {
            if provider.id == "copilot" {
                widgets::link(ui, &t!("copilot.sign_in_docs"), provider.site);
                widgets::link(ui, &t!("copilot.plans"), provider.key_url);
            } else {
                if !provider.key_url.is_empty() {
                    widgets::link(ui, &t!("setup.ai.get_key"), provider.key_url);
                }
                if !provider.site.is_empty() {
                    widgets::link(ui, &t!("setup.ai.prices"), provider.site);
                }
            }
        }
    });
    widgets::hint(ui, &t!("settings.ai.provider_hint"));
    ui.add_space(6.0);
}

/// Terms and privacy: when the student agreed, the texts, and the tour again.
/// Version, update checks and the source code.
fn about_section(ui: &mut egui::Ui, draft: &mut kg_engine::Settings) {
    section(ui, &t!("settings.about.title"), |ui| {
        widgets::hint(ui, &t!("app.version", version = env!("CARGO_PKG_VERSION")));
        ui.checkbox(&mut draft.check_updates, t!("settings.about.check_updates"))
            .on_hover_text(t!("settings.about.check_updates_hint"));
        let muted = crate::theme::palette(ui.ctx()).muted;
        crate::app::source_link(ui, muted);
        crate::app::company_link(ui, muted);
        widgets::hint(ui, &t!("brand.icon_credit"));
    });
}

fn legal_section(ui: &mut egui::Ui, engine: &Engine, actions: &mut Actions) {
    section(ui, &t!("settings.legal.title"), |ui| {
        let settings = engine.settings();
        let Some(region) = settings.region else {
            return;
        };
        widgets::hint(
            ui,
            &t!("settings.legal.region", region = i18n::region_name(region)),
        );
        if let Some(accepted) = &settings.terms_accepted {
            widgets::hint(
                ui,
                &t!(
                    "settings.legal.accepted",
                    date = accepted.accepted_at.as_str(),
                    version = accepted.version.as_str()
                ),
            );
        }
        let docs = crate::legal::LegalDoc::for_region(region);
        let id = egui::Id::new("settings_legal_open");
        let mut open: Option<usize> = ui.ctx().data(|d| d.get_temp(id)).flatten();
        ui.horizontal_wrapped(|ui| {
            for (n, doc) in docs.iter().enumerate() {
                if ui
                    .add(egui::Button::new(super::setup::doc_label(*doc)).selected(open == Some(n)))
                    .clicked()
                {
                    open = if open == Some(n) { None } else { Some(n) };
                }
            }
            if ui.button(t!("settings.legal.change_region")).clicked() {
                // Asks for the region and its terms again; nothing else changes.
                let mut settings = settings.clone();
                settings.region = None;
                actions.send(Command::SaveSettings(settings));
            }
            if ui.button(t!("settings.legal.tour")).clicked() {
                let mut settings = settings.clone();
                settings.tour_done = false;
                actions.send(Command::SaveSettings(settings));
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(id, open));
        if let Some(doc) = open.and_then(|n| docs.get(n)) {
            egui::ScrollArea::vertical()
                .id_salt("settings_legal_text")
                .max_height(360.0)
                .show(ui, |ui| {
                    crate::legal::show(ui, doc.text(&i18n::current(), region));
                });
        }
    });
}
