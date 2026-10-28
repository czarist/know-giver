//! Multiple-choice quiz, one question at a time.

use egui::{RichText, Stroke};
use kg_engine::{Command, QuizQuestion, QuizSession, QuizStep};
use rust_i18n::t;

use super::Actions;
use crate::app::{KnowGiverApp, Screen};
use crate::{theme, widgets};

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };

    let Some(session) = engine.quiz() else {
        widgets::header(ui, &t!("quiz.title"), &t!("quiz.subtitle"));
        widgets::hint(ui, &t!("quiz.none"));
        if ui.button(t!("quiz.go_to_plan")).clicked() {
            actions.go_to(Screen::Plan);
        }
        actions.run(app);
        return;
    };

    widgets::header(ui, &t!("quiz.title"), &session.topic_title);
    egui::ScrollArea::vertical().show(ui, |ui| match session.step {
        QuizStep::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(t!("quiz.loading"));
            });
        }
        QuizStep::Question { index, chosen } => {
            if let Some(question) = session.current() {
                question_view(ui, session, question, index, chosen, &mut actions);
            }
        }
        QuizStep::Finished { correct, total } => {
            finished(ui, session, correct, total, &mut actions);
        }
    });
    actions.run(app);
}

fn question_view(
    ui: &mut egui::Ui,
    session: &QuizSession,
    question: &QuizQuestion,
    index: usize,
    chosen: Option<usize>,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    let total = session.questions().len();
    ui.label(
        RichText::new(t!("quiz.progress", n = index + 1, total = total))
            .small()
            .color(p.muted),
    );
    ui.add(egui::ProgressBar::new(index as f32 / total.max(1) as f32).desired_height(4.0));
    if question.from_board {
        widgets::chip(ui, &t!("quiz.from_board"), p.accent);
    }
    ui.add_space(8.0);
    ui.label(RichText::new(&question.prompt).size(18.0));
    ui.add_space(8.0);

    for (i, option) in question.options.iter().enumerate() {
        let letter = char::from(b'A' + (i as u8 % 26));
        let (fill, stroke) = match chosen {
            Some(_) if i == question.correct => (p.success.gamma_multiply(0.15), p.success),
            Some(c) if c == i => (p.danger.gamma_multiply(0.15), p.danger),
            _ => (p.surface, p.border),
        };
        let button = egui::Button::new(RichText::new(format!("{letter})  {option}")))
            .fill(fill)
            .stroke(Stroke::new(1.0, stroke))
            .wrap()
            .min_size(egui::vec2(ui.available_width(), 40.0));
        if ui
            .add_enabled(
                chosen.is_none() || chosen == Some(i) || i == question.correct,
                button,
            )
            .clicked()
            && chosen.is_none()
        {
            actions.send(Command::AnswerQuiz(i));
        }
    }

    if let Some(choice) = chosen {
        ui.add_space(8.0);
        let (color, verdict) = if choice == question.correct {
            (p.success, t!("quiz.correct"))
        } else {
            (p.danger, t!("quiz.wrong"))
        };
        widgets::callout(ui, color, |ui| {
            ui.label(RichText::new(verdict).strong().color(color));
            if !question.explanation.is_empty() {
                ui.label(&question.explanation);
            }
        });
        ui.add_space(8.0);
        let next = if index + 1 < total {
            t!("quiz.next")
        } else {
            t!("quiz.see_result")
        };
        if widgets::primary_button(ui, &next).clicked() {
            actions.send(Command::NextQuestion);
        }
    }
}

fn finished(
    ui: &mut egui::Ui,
    session: &QuizSession,
    correct: u32,
    total: u32,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    widgets::card(ui, |ui| {
        ui.label(
            RichText::new(t!("quiz.score", correct = correct, total = total))
                .size(28.0)
                .color(p.accent),
        );
        let ratio = correct as f32 / total.max(1) as f32;
        let message = if ratio >= 0.9 {
            t!("quiz.result.great")
        } else if ratio >= 0.6 {
            t!("quiz.result.good")
        } else {
            t!("quiz.result.keep_going")
        };
        ui.label(message);
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if widgets::primary_button(ui, &t!("quiz.again")).clicked() {
            actions.send(Command::StartQuiz(session.topic_id));
        }
        if ui.button(t!("quiz.lesson")).clicked() {
            actions.send(Command::CloseQuiz);
            actions.send(Command::StartLesson(session.topic_id));
            actions.go_to(Screen::Lesson);
        }
        if ui.button(t!("quiz.go_to_plan")).clicked() {
            actions.send(Command::CloseQuiz);
            actions.go_to(Screen::Plan);
        }
    });
}
