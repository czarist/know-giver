//! Spaced-repetition reviews: due topics and the review flow.

use chrono::Days;
use egui::RichText;
use kg_engine::{Command, Engine, ReviewGrade, ReviewSession, ReviewStep};
use rust_i18n::t;

use super::Actions;
use crate::app::{KnowGiverApp, Screen};
use crate::{i18n, theme, widgets};

/// Days ahead listed under "coming up".
const UPCOMING_DAYS: u64 = 7;

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    let state = &mut app.ui;
    widgets::header(ui, &t!("review.title"), &t!("review.subtitle"));

    egui::ScrollArea::vertical().show(ui, |ui| {
        if let Some(session) = engine.review() {
            session_view(ui, session, &mut state.review_answer, &mut actions);
        } else {
            due_list(ui, engine, &mut actions);
        }
    });
    actions.run(app);
}

fn due_list(ui: &mut egui::Ui, engine: &Engine, actions: &mut Actions) {
    let Some(plan) = engine.active_plan() else {
        widgets::hint(ui, &t!("plan.empty"));
        if ui.button(t!("plan.go_to_edital")).clicked() {
            actions.go_to(Screen::Edital);
        }
        return;
    };
    let today = Engine::today();
    let due = plan.due_topics(today);
    if due.is_empty() {
        widgets::card(ui, |ui| {
            ui.label(RichText::new(t!("review.all_done")).strong());
            widgets::hint(ui, &t!("review.all_done_hint"));
        });
    } else {
        ui.label(RichText::new(t!("review.due_today", count = due.len())).strong());
    }
    for (subject, topic) in due {
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(&topic.title);
                    widgets::hint(ui, &subject.name);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t!("plan.lesson")).clicked() {
                        actions.send(Command::StartLesson(topic.id));
                        actions.go_to(Screen::Lesson);
                    }
                    if ui.button(t!("plan.quiz")).clicked() {
                        actions.send(Command::StartQuiz(topic.id));
                        actions.go_to(Screen::Quiz);
                    }
                    if widgets::primary_button(ui, &t!("review.start")).clicked() {
                        actions.send(Command::StartReview(topic.id));
                    }
                });
            });
        });
    }

    let horizon = today.checked_add_days(Days::new(UPCOMING_DAYS));
    let mut upcoming: Vec<_> = plan
        .topics()
        .filter(|(_, t)| {
            t.review
                .due
                .is_some_and(|d| d > today && horizon.is_some_and(|h| d <= h))
        })
        .collect();
    upcoming.sort_by_key(|(_, t)| t.review.due);
    if !upcoming.is_empty() {
        ui.add_space(16.0);
        ui.label(RichText::new(t!("review.upcoming")).strong());
        let muted = theme::palette(ui.ctx()).muted;
        for (subject, topic) in upcoming {
            ui.horizontal(|ui| {
                if let Some(due) = topic.review.due {
                    ui.label(RichText::new(i18n::date(due)).small().color(muted));
                }
                ui.label(&topic.title);
                ui.label(RichText::new(&subject.name).small().color(muted));
            });
        }
    }
}

fn session_view(
    ui: &mut egui::Ui,
    session: &ReviewSession,
    answer: &mut String,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(RichText::new(&session.topic_title).strong().size(18.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(t!("common.cancel")).clicked() {
                actions.send(Command::CloseReview);
            }
        });
    });

    match &session.step {
        ReviewStep::LoadingQuestion => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(t!("review.loading_question"));
            });
        }
        ReviewStep::Answering { question } => {
            widgets::card(ui, |ui| ui.label(RichText::new(question).size(17.0)));
            ui.add(
                egui::TextEdit::multiline(answer)
                    .hint_text(t!("review.answer_hint"))
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !answer.trim().is_empty(),
                        egui::Button::new(t!("review.check")),
                    )
                    .clicked()
                {
                    actions.send(Command::SubmitReviewAnswer(std::mem::take(answer)));
                }
            });
            ui.add_space(8.0);
            widgets::hint(ui, &t!("review.self_grade"));
            grade_buttons(ui, None, actions);
        }
        ReviewStep::Grading { question, answer } => {
            widgets::card(ui, |ui| ui.label(RichText::new(question).size(17.0)));
            widgets::hint(ui, answer);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(t!("review.grading"));
            });
        }
        ReviewStep::Graded {
            question,
            answer: given,
            feedback,
            suggested,
        } => {
            widgets::card(ui, |ui| ui.label(RichText::new(question).size(17.0)));
            widgets::hint(ui, given);
            let color = match suggested {
                ReviewGrade::Again => p.danger,
                ReviewGrade::Hard => p.warning,
                ReviewGrade::Good | ReviewGrade::Easy => p.success,
            };
            widgets::callout(ui, color, |ui| {
                ui.label(
                    RichText::new(t!("review.score", score = feedback.score))
                        .strong()
                        .color(color),
                );
                ui.label(&feedback.feedback);
            });
            ui.add_space(8.0);
            widgets::hint(ui, &t!("review.confirm_grade"));
            grade_buttons(ui, Some(*suggested), actions);
        }
    }
}

fn grade_buttons(ui: &mut egui::Ui, suggested: Option<ReviewGrade>, actions: &mut Actions) {
    ui.horizontal(|ui| {
        for grade in ReviewGrade::ALL {
            let label = i18n::grade(grade);
            let clicked = if Some(grade) == suggested {
                widgets::primary_button(ui, &label).clicked()
            } else {
                ui.button(label).clicked()
            };
            if clicked {
                actions.send(Command::GradeReview(grade));
            }
        }
    });
}
