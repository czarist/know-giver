//! The active study plan, by week or by subject.

use egui::RichText;
use kg_engine::{Command, Difficulty, Engine, StudyPlan, Topic, TopicStatus};
use rust_i18n::t;

use super::{Actions, current_week};
use crate::app::{KnowGiverApp, PlanView, Screen};
use crate::{i18n, theme, widgets};

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    let state = &mut app.ui;

    let Some(plan) = engine.active_plan() else {
        widgets::header(ui, &t!("plan.title"), &t!("plan.subtitle"));
        widgets::hint(ui, &t!("plan.empty"));
        if ui.button(t!("plan.go_to_edital")).clicked() {
            actions.go_to(Screen::Edital);
        }
        actions.run(app);
        return;
    };

    ui.heading(&plan.title);
    if !plan.exam.is_empty() {
        widgets::hint(ui, &plan.exam);
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t!(
                "plan.summary",
                weeks = plan.weeks,
                topics = plan.topic_count(),
                done = plan.count_with_status(TopicStatus::Done)
            ))
            .small(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.selectable_value(
                &mut state.plan_view,
                PlanView::BySubject,
                t!("plan.by_subject"),
            );
            ui.selectable_value(&mut state.plan_view, PlanView::ByWeek, t!("plan.by_week"));
            if engine.plans().len() > 1 {
                egui::ComboBox::from_id_salt("plan_picker")
                    .selected_text(t!("plan.switch"))
                    .show_ui(ui, |ui| {
                        for summary in engine.plans() {
                            if ui
                                .selectable_label(summary.id == plan.id, &summary.title)
                                .clicked()
                            {
                                actions.send(Command::SelectPlan(summary.id));
                            }
                        }
                    });
            }
        });
    });
    ui.add(egui::ProgressBar::new(plan.progress()).desired_height(6.0));
    ui.add_space(8.0);

    egui::ScrollArea::vertical().show(ui, |ui| match state.plan_view {
        PlanView::ByWeek => by_week(ui, plan, &mut actions),
        PlanView::BySubject => by_subject(ui, plan, &mut actions),
    });
    actions.run(app);
}

fn by_week(ui: &mut egui::Ui, plan: &StudyPlan, actions: &mut Actions) {
    let current = current_week(plan);
    for week in plan.week_numbers() {
        let mut title = t!("plan.week", n = week).into_owned();
        if week == current {
            title = format!("{title}  ·  {}", t!("plan.this_week"));
        }
        egui::CollapsingHeader::new(RichText::new(title).strong())
            .id_salt(("week", week))
            .default_open(week == current)
            .show(ui, |ui| {
                for (subject, topic) in plan.topics().filter(|(_, t)| t.week == week) {
                    topic_row(ui, Some(&subject.name), topic, actions);
                }
            });
    }
}

fn by_subject(ui: &mut egui::Ui, plan: &StudyPlan, actions: &mut Actions) {
    for subject in &plan.subjects {
        let title = format!("{}  ·  {:.0}%", subject.name, subject.progress() * 100.0);
        egui::CollapsingHeader::new(RichText::new(title).strong())
            .id_salt(("subject", subject.id.0))
            .default_open(true)
            .show(ui, |ui| {
                for topic in &subject.topics {
                    topic_row(ui, None, topic, actions);
                }
            });
    }
}

pub fn difficulty_color(ui: &egui::Ui, difficulty: Difficulty) -> egui::Color32 {
    let p = theme::palette(ui.ctx());
    match difficulty {
        Difficulty::Easy => p.success,
        Difficulty::Medium => p.warning,
        Difficulty::Hard => p.danger,
    }
}

fn topic_row(ui: &mut egui::Ui, subject: Option<&str>, topic: &Topic, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    let today = Engine::today();
    ui.horizontal(|ui| {
        let mut status = topic.status;
        egui::ComboBox::from_id_salt(("status", topic.id.0))
            .width(120.0)
            .selected_text(RichText::new(i18n::status(status)).small())
            .show_ui(ui, |ui| {
                for option in TopicStatus::ALL {
                    ui.selectable_value(&mut status, option, i18n::status(option));
                }
            });
        if status != topic.status {
            actions.send(Command::SetTopicStatus(topic.id, status));
        }

        ui.vertical(|ui| {
            let title = RichText::new(&topic.title);
            ui.label(if topic.status == TopicStatus::Done {
                title.color(p.muted)
            } else {
                title
            });
            if let Some(subject) = subject {
                ui.label(RichText::new(subject).small().color(p.muted));
            }
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button(t!("plan.lesson")).clicked() {
                actions.send(Command::StartLesson(topic.id));
                actions.go_to(Screen::Lesson);
            }
            if ui.small_button(t!("plan.quiz")).clicked() {
                actions.send(Command::StartQuiz(topic.id));
                actions.go_to(Screen::Quiz);
            }
            if topic.is_due(today) {
                widgets::chip(ui, &t!("plan.review_due"), p.accent);
            }
            widgets::chip(
                ui,
                &i18n::difficulty(topic.difficulty),
                difficulty_color(ui, topic.difficulty),
            );
        });
    });
    ui.separator();
}
