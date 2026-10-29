//! The active study plan, by week or by subject.

use egui::RichText;
use kg_engine::{Command, Difficulty, Engine, HandoutScope, StudyPlan, Topic, TopicStatus};
use rust_i18n::t;

use super::{Actions, current_week, handout, lesson_button};
use crate::app::{KnowGiverApp, PlanView, Screen};
use crate::icons::Icon;
use crate::{i18n, theme, widgets, working};

/// A handout the user asked for while the screen was drawn.
#[derive(Debug, Default)]
struct HandoutAsk(Option<(HandoutScope, bool)>);

impl HandoutAsk {
    /// Handout button with a right-click "write again" option.
    fn button(&mut self, ui: &mut egui::Ui, label: &str, scope: HandoutScope, enabled: bool) {
        let response = ui
            .add_enabled(enabled, egui::Button::new(label).small())
            .on_hover_text(t!("plan.handout_hint"));
        if response.clicked() {
            self.0 = Some((scope, false));
        }
        response.context_menu(|ui| {
            if ui.button(t!("plan.handout_regenerate")).clicked() {
                self.0 = Some((scope, true));
                ui.close();
            }
        });
    }
}

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let mut ask = HandoutAsk::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    let state = &mut app.ui;

    let Some(plan) = engine.active_plan() else {
        if widgets::empty_state(
            ui,
            Icon::Plan,
            &t!("plan.title"),
            &t!("plan.empty"),
            Some(&t!("plan.go_to_edital")),
        ) {
            actions.go_to(Screen::Edital);
        }
        actions.run(app);
        return;
    };
    let idle = engine.handout_job().is_none() && engine.prepare_job().is_none();

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

    if let Some(job) = engine.prepare_job() {
        ui.add_space(8.0);
        prepare_panel(ui, engine, job, &mut actions);
    } else {
        handout_bar(ui, engine, plan, &mut ask, &mut actions);
    }
    ui.add_space(4.0);

    egui::ScrollArea::vertical().show(ui, |ui| match state.plan_view {
        PlanView::ByWeek => by_week(ui, engine, plan, idle, &mut ask, &mut actions),
        PlanView::BySubject => by_subject(ui, engine, plan, idle, &mut ask, &mut actions),
    });

    if let Some((scope, regenerate)) = ask.0
        && let Some(request) = handout::request(plan, scope, regenerate)
    {
        actions.send(Command::ExportHandout(request));
    }
    actions.run(app);
}

/// Whole-plan handout button, or progress while a handout is being written.
fn handout_bar(
    ui: &mut egui::Ui,
    engine: &Engine,
    plan: &StudyPlan,
    ask: &mut HandoutAsk,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    ui.horizontal(|ui| match engine.handout_job() {
        Some(job) => {
            ui.spinner();
            ui.label(t!(
                "plan.handout_progress",
                done = job.done(),
                total = job.total
            ));
            ui.add(
                egui::ProgressBar::new(job.done() as f32 / job.total.max(1) as f32)
                    .desired_width(180.0)
                    .desired_height(6.0),
            );
            if ui.small_button(t!("common.cancel")).clicked() {
                actions.send(Command::CancelHandout);
            }
        }
        None => {
            ui.add(Icon::Handout.image(18.0, p.accent));
            ask.button(ui, &t!("plan.handout_all"), HandoutScope::Plan, true);
            if ui
                .button(t!("plan.prepare_all"))
                .on_hover_text(t!("plan.prepare_all_hint"))
                .clicked()
            {
                actions.send(Command::PrepareContent(handout::prepare_request()));
            }
            let missing = plan.topic_count().saturating_sub(engine.handouts_cached());
            let note = if missing == 0 {
                t!("plan.handout_all_ready")
            } else {
                t!("plan.handout_missing", count = missing)
            };
            ui.label(RichText::new(note).small().color(p.muted));
        }
    });
}

/// Progress of "prepare everything", with the thinking teacher.
fn prepare_panel(
    ui: &mut egui::Ui,
    engine: &Engine,
    job: &kg_engine::PrepareJob,
    actions: &mut Actions,
) {
    let done = job.done();
    let working_on = job.working_on();
    let detail = if job.is_cancelling() {
        t!("plan.prepare_cancelling").into_owned()
    } else if let Some(pause) = job.paused_for() {
        t!("plan.prepare_paused", seconds = pause.as_secs() + 1).into_owned()
    } else if working_on.is_empty() {
        t!("plan.prepare_saving").into_owned()
    } else {
        t!("plan.prepare_writing", topics = working_on.join(" · ")).into_owned()
    };
    let mut note = t!("plan.prepare_count", done = done, total = job.total).into_owned();
    match job.remaining() {
        Some(left) => {
            note.push_str(" · ");
            note.push_str(&t!(
                "plan.prepare_left",
                time = working::duration_text(left)
            ));
        }
        None => {
            note.push_str(" · ");
            note.push_str(&t!("plan.prepare_long"));
        }
    }
    if job.failed() > 0 {
        note.push_str(" · ");
        note.push_str(&t!("plan.prepare_failed", count = job.failed()));
    }
    working::panel(
        ui,
        engine,
        &working::Working {
            title: &t!("plan.prepare_title"),
            detail: &detail,
            note: &note,
            progress: Some(done as f32 / job.total.max(1) as f32),
        },
        |ui| {
            ui.horizontal(|ui| {
                widgets::hint(ui, &t!("plan.prepare_keep_using"));
                if !job.is_cancelling() && ui.small_button(t!("common.cancel")).clicked() {
                    actions.send(Command::CancelPrepare);
                }
            });
        },
    );
}

fn by_week(
    ui: &mut egui::Ui,
    engine: &Engine,
    plan: &StudyPlan,
    idle: bool,
    ask: &mut HandoutAsk,
    actions: &mut Actions,
) {
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
                    topic_row(ui, engine, Some(&subject.name), topic, idle, ask, actions);
                }
            });
    }
}

fn by_subject(
    ui: &mut egui::Ui,
    engine: &Engine,
    plan: &StudyPlan,
    idle: bool,
    ask: &mut HandoutAsk,
    actions: &mut Actions,
) {
    for subject in &plan.subjects {
        let title = format!("{}  ·  {:.0}%", subject.name, subject.progress() * 100.0);
        egui::CollapsingHeader::new(RichText::new(title).strong())
            .id_salt(("subject", subject.id.0))
            .default_open(true)
            .show(ui, |ui| {
                ask.button(
                    ui,
                    &t!("plan.handout_subject"),
                    HandoutScope::Subject(subject.id),
                    idle,
                );
                for topic in &subject.topics {
                    topic_row(ui, engine, None, topic, idle, ask, actions);
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

fn topic_row(
    ui: &mut egui::Ui,
    engine: &Engine,
    subject: Option<&str>,
    topic: &Topic,
    idle: bool,
    ask: &mut HandoutAsk,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    let today = Engine::today();
    let emphasis = engine.board_emphasis(topic.id);
    let mistakes = engine
        .mistakes()
        .iter()
        .filter(|m| m.topic_id == topic.id)
        .count();
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

        // Buttons and chips first, from the right; the title takes what is
        // left, cut with an ellipsis and shown whole on hover.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            lesson_button(ui, engine, topic, actions);
            if ui.small_button(t!("plan.quiz")).clicked() {
                actions.send(Command::StartQuiz(topic.id));
                actions.go_to(Screen::Quiz);
            }
            ask.button(ui, &t!("plan.handout"), HandoutScope::Topic(topic.id), idle);
            if topic.is_due(today) {
                widgets::chip(ui, &t!("plan.review_due"), p.accent);
            }
            if mistakes > 0 {
                widgets::chip(ui, &t!("plan.mistakes", count = mistakes), p.danger);
            }
            if emphasis > 0 {
                widgets::chip(ui, &t!("plan.board_count", count = emphasis), p.muted);
            }
            widgets::chip(
                ui,
                &i18n::difficulty(topic.difficulty),
                difficulty_color(ui, topic.difficulty),
            );
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.vertical(|ui| {
                    let title = RichText::new(&topic.title);
                    let title = if topic.status == TopicStatus::Done {
                        title.color(p.muted)
                    } else {
                        title
                    };
                    ui.horizontal(|ui| {
                        if topic.lesson_taken() {
                            ui.add(Icon::Check.image(14.0, p.success))
                                .on_hover_text(t!("plan.lesson_taken"));
                        }
                        ui.add(egui::Label::new(title).truncate())
                            .on_hover_text(&topic.title);
                    });
                    if let Some(subject) = subject {
                        ui.add(
                            egui::Label::new(RichText::new(subject).small().color(p.muted))
                                .truncate(),
                        );
                    }
                });
            });
        });
    });
    ui.separator();
}
