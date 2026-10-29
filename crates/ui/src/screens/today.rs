//! "Today": where the app opens. What to study now, in one click.

use chrono::Timelike;
use egui::{Color32, RichText};
use kg_engine::{Command, Engine, StudyPlan, TopicStatus};
use rust_i18n::t;

use super::{Actions, current_week};
use crate::app::{KnowGiverApp, Screen};
use crate::art::Art;
use crate::icons::Icon;
use crate::{i18n, theme, widgets};

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    let Some(plan) = engine.active_plan() else {
        if widgets::empty_state(
            ui,
            Icon::Edital,
            &t!("today.no_plan_title"),
            &t!("today.no_plan"),
            Some(&t!("edital.select_pdf")),
        ) {
            actions.go_to(Screen::Edital);
        }
        actions.run(app);
        return;
    };

    let greeting = match chrono::Local::now().hour() {
        5..=11 => t!("today.greeting.morning"),
        12..=17 => t!("today.greeting.afternoon"),
        _ => t!("today.greeting.evening"),
    };
    let name = engine.settings().student_name.trim();
    let title = if name.is_empty() {
        greeting.into_owned()
    } else {
        t!("today.greeting_named", greeting = greeting, name = name).into_owned()
    };
    widgets::cover(
        ui,
        &plan.title,
        &title,
        &t!("today.subtitle", date = i18n::date(Engine::today())),
        Art::Mark,
        150.0,
        |_| {},
    );
    ui.add_space(14.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        stats(ui, engine, plan);
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(t!(
                    "today.study_time",
                    duration = study_duration(engine.today_study_seconds())
                ))
                .strong(),
            );
            widgets::hint(ui, &t!("today.study_time_hint"));
        });
        ui.add_space(14.0);
        next_step(ui, engine, plan, &mut actions);
        ui.add_space(14.0);
        this_week(ui, plan, &mut actions);
    });
    actions.run(app);
}

fn study_duration(seconds: u64) -> String {
    let minutes = seconds / 60;
    if seconds == 0 {
        t!("today.study_zero").into_owned()
    } else if minutes == 0 {
        t!("today.study_under_minute").into_owned()
    } else if minutes < 60 {
        t!("today.study_minutes", minutes = minutes).into_owned()
    } else {
        t!(
            "today.study_hours",
            hours = minutes / 60,
            minutes = minutes % 60
        )
        .into_owned()
    }
}

fn stats(ui: &mut egui::Ui, engine: &Engine, plan: &StudyPlan) {
    let p = theme::palette(ui.ctx());
    let due = plan.due_topics(Engine::today()).len();
    let streak = engine.streak();
    let streak_note = if engine.studied_today() {
        t!("today.streak_kept")
    } else if streak > 0 {
        t!("today.streak_keep")
    } else {
        t!("today.streak_start")
    };
    let progress =
        ui.ctx()
            .animate_value_with_time(ui.id().with("today_progress"), plan.progress(), 0.6);
    let columns = 3.0;
    let width = (ui.available_width() - 2.0 * 12.0) / columns;
    ui.horizontal_top(|ui| {
        stat_card(
            ui,
            width,
            Icon::Streak,
            p.highlight,
            &t!("today.streak", count = streak),
            &streak_note,
        );
        stat_card(
            ui,
            width,
            Icon::Review,
            p.accent,
            &t!("today.due", count = due),
            &t!("today.due_note"),
        );
        stat_card(
            ui,
            width,
            Icon::Progress,
            p.accent,
            &format!("{:.0}%", progress * 100.0),
            &t!("today.week", n = current_week(plan), total = plan.weeks),
        );
    });
}

fn stat_card(ui: &mut egui::Ui, width: f32, icon: Icon, color: Color32, value: &str, note: &str) {
    let p = theme::palette(ui.ctx());
    egui::Frame::new()
        .fill(p.surface)
        .stroke(egui::Stroke::new(1.2, p.ink.gamma_multiply(0.85)))
        .corner_radius(0)
        .inner_margin(egui::Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width(width - 34.0);
            ui.set_min_height(50.0);
            ui.horizontal(|ui| {
                ui.add(icon.image(26.0, color));
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(value)
                            .family(theme::heading_family())
                            .size(22.0)
                            .color(color),
                    );
                    ui.label(RichText::new(note).small().color(p.muted));
                });
            });
        });
}

/// The single most useful thing to do now: reviews first, then the next
/// lesson of the week.
fn next_step(ui: &mut egui::Ui, engine: &Engine, plan: &StudyPlan, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    let due = plan.due_topics(Engine::today()).len();
    let week = current_week(plan);
    let next = plan
        .topics()
        .filter(|(_, t)| !t.lesson_taken())
        .min_by_key(|(_, t)| (t.week > week, t.status != TopicStatus::Studying, t.week));

    widgets::card(ui, |ui| {
        ui.label(RichText::new(t!("today.next_step")).small().color(p.muted));
        if due > 0 {
            ui.label(
                RichText::new(t!("today.next.review", count = due))
                    .family(theme::heading_family())
                    .size(20.0),
            );
            widgets::hint(ui, &t!("today.next.review_note"));
            if widgets::primary_button(ui, &t!("today.next.review_action")).clicked() {
                actions.go_to(Screen::Review);
            }
            return;
        }
        let Some((subject, topic)) = next else {
            ui.label(
                RichText::new(t!("today.all_done"))
                    .family(theme::heading_family())
                    .size(20.0),
            );
            return;
        };
        ui.label(
            RichText::new(&topic.title)
                .family(theme::heading_family())
                .size(20.0),
        );
        widgets::hint(ui, &subject.name);
        let emphasis = engine.board_emphasis(topic.id);
        if emphasis > 0 {
            widgets::chip(ui, &t!("plan.board_count", count = emphasis), p.highlight);
        }
        ui.horizontal(|ui| {
            let start = egui::Button::image_and_text(
                Icon::Play.image(16.0, Color32::WHITE),
                RichText::new(t!("lesson.start")).color(Color32::WHITE),
            )
            .fill(p.accent);
            if ui.add(start).clicked() {
                actions.send(Command::StartLesson(topic.id));
                actions.go_to(Screen::Lesson);
            }
            if ui.button(t!("plan.quiz")).clicked() {
                actions.send(Command::StartQuiz(topic.id));
                actions.go_to(Screen::Quiz);
            }
        });
    });
}

fn this_week(ui: &mut egui::Ui, plan: &StudyPlan, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    let week = current_week(plan);
    ui.label(
        RichText::new(t!("plan.week", n = week))
            .family(theme::heading_family())
            .size(18.0),
    );
    for (subject, topic) in plan.topics().filter(|(_, t)| t.week == week) {
        ui.horizontal(|ui| {
            let taken = topic.lesson_taken();
            let color = if taken { p.success } else { p.border };
            let check = ui.add(Icon::Check.image(18.0, color));
            if taken {
                check.on_hover_text(t!("plan.lesson_taken"));
            }
            ui.vertical(|ui| {
                ui.label(&topic.title);
                ui.label(RichText::new(&subject.name).small().color(p.muted));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if taken {
                    t!("plan.lesson_again")
                } else {
                    t!("plan.lesson")
                };
                if ui.small_button(label).clicked() {
                    actions.send(Command::StartLesson(topic.id));
                    actions.go_to(Screen::Lesson);
                }
                widgets::chip(ui, &i18n::status(topic.status), p.muted);
            });
        });
    }
}
