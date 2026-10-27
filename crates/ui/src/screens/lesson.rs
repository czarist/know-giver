//! Voice lesson: transcript, microphone orb and typed fallback.

use egui::RichText;
use kg_engine::{Command, Engine, LessonPhase, LessonSession, Speaker, TopicStatus};
use rust_i18n::t;

use super::{Actions, current_week};
use crate::app::{KnowGiverApp, Screen};
use crate::{i18n, theme, widgets};

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    let state = &mut app.ui;

    let Some(lesson) = engine.lesson() else {
        no_lesson(ui, engine, &mut actions);
        actions.run(app);
        return;
    };

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.heading(&lesson.topic_title);
            widgets::hint(ui, &lesson.subject);
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(t!("lesson.end")).clicked() {
                actions.send(Command::EndLesson);
            }
        });
    });

    model_warnings(ui, engine, &mut actions);

    let input_height = 150.0;
    egui::ScrollArea::vertical()
        .max_height((ui.available_height() - input_height).max(120.0))
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| transcript(ui, lesson));

    ui.separator();
    controls(ui, engine, lesson, &mut state.lesson_input, &mut actions);
    actions.run(app);
}

fn no_lesson(ui: &mut egui::Ui, engine: &Engine, actions: &mut Actions) {
    widgets::header(ui, &t!("lesson.title"), &t!("lesson.subtitle"));
    let Some(plan) = engine.active_plan() else {
        widgets::hint(ui, &t!("plan.empty"));
        if ui.button(t!("plan.go_to_edital")).clicked() {
            actions.go_to(Screen::Edital);
        }
        return;
    };
    widgets::hint(ui, &t!("lesson.none"));
    ui.add_space(8.0);
    ui.label(RichText::new(t!("lesson.suggestions")).strong());
    let week = current_week(plan);
    let suggestions = plan
        .topics()
        .filter(|(_, t)| t.status != TopicStatus::Done && t.week <= week)
        .take(6);
    for (subject, topic) in suggestions {
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(&topic.title);
                    widgets::hint(ui, &subject.name);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::primary_button(ui, &t!("lesson.start")).clicked() {
                        actions.send(Command::StartLesson(topic.id));
                    }
                });
            });
        });
    }
}

fn model_warnings(ui: &mut egui::Ui, engine: &Engine, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    let mut warn = |ui: &mut egui::Ui, text: &str| {
        widgets::callout(ui, p.warning, |ui| {
            ui.horizontal(|ui| {
                ui.label(text);
                if ui.small_button(t!("lesson.open_settings")).clicked() {
                    actions.go_to(Screen::Settings);
                }
            });
        });
    };
    if !engine.stt_ready() {
        warn(ui, &t!("lesson.need_stt"));
    }
    if engine.settings().voice_enabled && !engine.tts_ready() {
        warn(ui, &t!("lesson.need_tts"));
    }
}

fn transcript(ui: &mut egui::Ui, lesson: &LessonSession) {
    let p = theme::palette(ui.ctx());
    for turn in &lesson.turns {
        let (who, fill) = match turn.speaker {
            Speaker::Tutor => (t!("lesson.tutor"), p.surface),
            Speaker::Student => (t!("lesson.you"), p.accent.gamma_multiply(0.10)),
        };
        ui.label(RichText::new(who).small().strong().color(p.muted));
        egui::Frame::new()
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, p.border))
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(&turn.text).size(16.5));
            });
        ui.add_space(4.0);
    }
    if matches!(
        lesson.phase,
        LessonPhase::Thinking | LessonPhase::Transcribing
    ) {
        ui.horizontal(|ui| {
            ui.spinner();
            widgets::hint(ui, &i18n::phase(lesson.phase));
        });
    }
}

fn controls(
    ui: &mut egui::Ui,
    engine: &Engine,
    lesson: &LessonSession,
    input: &mut String,
    actions: &mut Actions,
) {
    let busy = matches!(
        lesson.phase,
        LessonPhase::Thinking | LessonPhase::Transcribing
    );
    ui.horizontal(|ui| {
        let orb = widgets::voice_orb(
            ui,
            lesson.phase,
            lesson.input_level(),
            engine.stt_ready() && !busy,
        );
        if orb.clicked() {
            match lesson.phase {
                LessonPhase::Idle => actions.send(Command::StartListening),
                LessonPhase::Listening => actions.send(Command::StopListening),
                LessonPhase::Speaking => actions.send(Command::StopSpeaking),
                LessonPhase::Thinking | LessonPhase::Transcribing => {}
            }
        }

        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new(i18n::phase(lesson.phase)).size(17.0));
            let hint = match lesson.phase {
                LessonPhase::Idle => t!("lesson.orb_hint.idle"),
                LessonPhase::Listening => t!("lesson.orb_hint.listening"),
                LessonPhase::Speaking => t!("lesson.orb_hint.speaking"),
                LessonPhase::Thinking | LessonPhase::Transcribing => t!("lesson.orb_hint.wait"),
            };
            widgets::hint(ui, &hint);

            ui.horizontal(|ui| {
                let field = ui.add_enabled(
                    !busy,
                    egui::TextEdit::singleline(input)
                        .hint_text(t!("lesson.type_hint"))
                        .desired_width((ui.available_width() - 110.0).max(160.0)),
                );
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let send = ui
                    .add_enabled(
                        !busy && !input.trim().is_empty(),
                        egui::Button::new(t!("lesson.send")),
                    )
                    .clicked();
                if (enter || send) && !input.trim().is_empty() {
                    actions.send(Command::SendMessage(std::mem::take(input)));
                    field.request_focus();
                }
            });
        });
    });
}
