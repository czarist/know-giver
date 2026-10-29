//! Multiple-choice quiz, one question at a time.

use egui::{RichText, Stroke};
use kg_engine::{Command, Engine, Mistake, QuizQuestion, QuizSession, QuizStep, TopicId};
use rust_i18n::t;

use super::Actions;
use crate::app::{KnowGiverApp, Screen};
use crate::icons::Icon;
use crate::{i18n, theme, widgets};

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };

    let Some(session) = engine.quiz() else {
        if !engine.mistakes().is_empty() {
            widgets::header(ui, &t!("quiz.title"), &t!("quiz.notebook_hint"));
            egui::ScrollArea::vertical().show(ui, |ui| notebook(ui, engine, &mut actions));
            actions.run(app);
            return;
        }
        if widgets::empty_state(
            ui,
            Icon::Quiz,
            &t!("quiz.title"),
            &t!("quiz.none"),
            Some(&t!("quiz.go_to_plan")),
        ) {
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
            finished(ui, engine, session, correct, total, &mut actions);
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
    ui.horizontal(|ui| {
        if question.from_board {
            widgets::chip(ui, &t!("quiz.from_board"), p.accent);
        }
        if session.is_retry(index) {
            widgets::chip(ui, &t!("quiz.retry_badge"), p.warning);
        }
    });
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

/// Questions answered wrong, by topic, with the way to try them again.
fn notebook(ui: &mut egui::Ui, engine: &Engine, actions: &mut Actions) {
    let Some(plan) = engine.active_plan() else {
        return;
    };
    ui.label(
        RichText::new(t!("quiz.notebook"))
            .family(theme::heading_family())
            .size(18.0),
    );
    ui.add_space(6.0);
    for (subject, topic) in plan.topics() {
        let mistakes: Vec<&Mistake> = engine
            .mistakes()
            .iter()
            .filter(|m| m.topic_id == topic.id)
            .collect();
        if mistakes.is_empty() {
            continue;
        }
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&topic.title).strong());
                    widgets::hint(ui, &subject.name);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::primary_button(
                        ui,
                        &t!("quiz.retry_mistakes", count = mistakes.len()),
                    )
                    .clicked()
                    {
                        actions.send(Command::RetryMistakes(topic.id));
                    }
                });
            });
            for mistake in mistakes {
                mistake_row(ui, topic.id, mistake, actions);
            }
        });
        ui.add_space(4.0);
    }
    ui.add_space(8.0);
    if ui.button(t!("quiz.go_to_plan")).clicked() {
        actions.go_to(Screen::Plan);
    }
}

/// One missed question: folded to its prompt, open to both answers.
fn mistake_row(ui: &mut egui::Ui, topic: TopicId, mistake: &Mistake, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    let q = &mistake.question;
    let prompt: String = q.prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    egui::CollapsingHeader::new(prompt)
        .id_salt(("mistake", topic.0, mistake.id.0))
        .show(ui, |ui| {
            let option = |i: usize| q.options.get(i).map_or("", String::as_str);
            if mistake.chosen != q.correct {
                ui.label(
                    RichText::new(format!(
                        "{}: {}",
                        t!("quiz.your_answer"),
                        option(mistake.chosen)
                    ))
                    .color(p.danger),
                );
            }
            ui.label(
                RichText::new(format!(
                    "{}: {}",
                    t!("quiz.right_answer"),
                    option(q.correct)
                ))
                .color(p.success),
            );
            if !q.explanation.trim().is_empty() {
                widgets::hint(ui, q.explanation.trim());
            }
            ui.horizontal(|ui| {
                widgets::hint(
                    ui,
                    &t!(
                        "quiz.missed_on",
                        date = i18n::date(mistake.missed_on),
                        count = mistake.misses
                    ),
                );
                if ui.small_button(t!("quiz.remove_mistake")).clicked() {
                    actions.send(Command::DeleteMistake(mistake.id));
                }
            });
        });
}

fn finished(
    ui: &mut egui::Ui,
    engine: &Engine,
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
        let mistakes = engine
            .mistakes()
            .iter()
            .filter(|m| m.topic_id == session.topic_id)
            .count();
        if mistakes > 0
            && ui
                .button(t!("quiz.retry_mistakes", count = mistakes))
                .clicked()
        {
            actions.send(Command::RetryMistakes(session.topic_id));
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
