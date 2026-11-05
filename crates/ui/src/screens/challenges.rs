//! Challenges of the active plan and the sticker showcase. Everything shown
//! comes from the engine; this screen only draws it and collects clicks.

use egui::RichText;
use kg_engine::{
    ChallengeId, ChallengeProgress, ChallengeUnlock, Command, Engine, NextStep, StudyPlan,
};
use rust_i18n::t;

use super::Actions;
use crate::app::{KnowGiverApp, Screen};
use crate::{i18n, stickers, theme, widgets};

/// Width of a showcase card.
const SHOWCASE_CARD: f32 = 196.0;

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    widgets::header(ui, &t!("challenges.title"), &t!("challenges.subtitle"));
    egui::ScrollArea::vertical().show(ui, |ui| {
        match engine.active_plan() {
            Some(plan) => plan_challenges(ui, engine, plan, &mut actions),
            None => widgets::hint(ui, &t!("challenges.no_plan")),
        }
        ui.add_space(20.0);
        showcase(ui, engine);
    });
    actions.run(app);
}

fn plan_challenges(ui: &mut egui::Ui, engine: &Engine, plan: &StudyPlan, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    widgets::kicker(ui, &t!("challenges.plan_section"), p.muted);
    ui.label(
        RichText::new(&plan.title)
            .family(theme::heading_family())
            .size(18.0),
    );
    ui.add_space(8.0);
    for challenge in engine.challenges() {
        challenge_card(ui, engine, plan, challenge, actions);
        ui.add_space(10.0);
    }
}

fn challenge_card(
    ui: &mut egui::Ui,
    engine: &Engine,
    plan: &StudyPlan,
    challenge: &ChallengeProgress,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    let won_here = engine
        .sticker_unlocks()
        .iter()
        .find(|u| u.challenge == challenge.id && u.plan == Some(plan.id));
    widgets::card(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.add(stickers::image(
                ui.ctx(),
                challenge.id,
                64.0,
                challenge.is_met(),
            ));
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(i18n::challenge_name(challenge.id))
                            .family(theme::heading_family())
                            .size(18.0),
                    );
                    if challenge.is_met() {
                        widgets::chip(ui, &t!("challenges.done"), p.success);
                    }
                });
                ui.label(i18n::challenge_goal(challenge.id));
                let mut scope = t!("challenges.valid_in", plan = plan.title).into_owned();
                if let Some(week) = challenge.week {
                    scope = format!("{scope} · {}", t!("challenges.week", n = week));
                }
                ui.label(RichText::new(scope).small().color(p.muted));
                ui.add_space(4.0);
                widgets::progress_line(
                    ui,
                    240.0,
                    challenge.fraction(),
                    &t!(
                        "challenges.progress",
                        current = challenge.current,
                        target = challenge.target
                    ),
                    if challenge.is_met() {
                        p.success
                    } else {
                        p.accent
                    },
                );
                ui.label(
                    RichText::new(t!(
                        "challenges.reward",
                        name = i18n::challenge_name(challenge.id)
                    ))
                    .small()
                    .color(p.highlight),
                );
                if let Some(unlock) = won_here {
                    ui.label(RichText::new(won_on(unlock)).small().color(p.muted));
                }
                if challenge.id == ChallengeId::Comeback && !challenge.is_met() {
                    ui.label(
                        RichText::new(t!("challenges.comeback_note"))
                            .small()
                            .italics()
                            .color(p.muted),
                    );
                }
                if !challenge.is_met() {
                    next_step(ui, engine, plan, challenge.next, actions);
                }
            });
        });
    });
}

/// The topic to go to and a button that opens it.
fn next_step(
    ui: &mut egui::Ui,
    engine: &Engine,
    plan: &StudyPlan,
    next: NextStep,
    actions: &mut Actions,
) {
    let topic_id = match next {
        NextStep::Lesson(id) | NextStep::Quiz(id) | NextStep::Mistakes(id) => id,
        NextStep::None => return,
    };
    let Some((subject, topic)) = plan.find_topic(topic_id) else {
        return;
    };
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        let (label, command, screen) = match next {
            // Always a lesson that counts: continue one left halfway or
            // start one, never the recap of a lesson already taken.
            NextStep::Lesson(id) if engine.has_lesson_draft(id) => (
                t!("lesson.continue").into_owned(),
                Command::ResumeLesson(id),
                Screen::Lesson,
            ),
            NextStep::Lesson(id) => (
                t!("lesson.start").into_owned(),
                Command::StartLesson(id),
                Screen::Lesson,
            ),
            NextStep::Quiz(id) => (
                t!("challenges.open_quiz").into_owned(),
                Command::StartQuiz(id),
                Screen::Quiz,
            ),
            NextStep::Mistakes(id) => (
                t!("challenges.open_mistakes").into_owned(),
                Command::RetryMistakes(id),
                Screen::Quiz,
            ),
            NextStep::None => return,
        };
        if widgets::primary_button(ui, &label).clicked() {
            actions.send(command);
            actions.go_to(screen);
        }
        widgets::hint(
            ui,
            &t!(
                "challenges.topic",
                topic = topic.title,
                subject = subject.name
            ),
        );
    });
}

fn won_on(unlock: &ChallengeUnlock) -> String {
    if unlock.date_known {
        t!("challenges.won_on", date = i18n::date(unlock.unlocked_on)).into_owned()
    } else {
        t!(
            "challenges.won_noted",
            date = i18n::date(unlock.unlocked_on)
        )
        .into_owned()
    }
}

/// One card per sticker, Steam-style: colour when won anywhere, grey with
/// the requirement until then.
fn showcase(ui: &mut egui::Ui, engine: &Engine) {
    let p = theme::palette(ui.ctx());
    let unlocks = engine.sticker_unlocks();
    let won = ChallengeId::ALL
        .iter()
        .filter(|id| unlocks.iter().any(|u| u.challenge == **id))
        .count();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t!("challenges.showcase"))
                .family(theme::heading_family())
                .size(20.0),
        );
        ui.label(
            RichText::new(t!(
                "challenges.showcase_count",
                won = won,
                total = ChallengeId::ALL.len()
            ))
            .color(p.muted),
        );
    });
    widgets::hint(ui, &t!("challenges.showcase_note"));
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
        for id in ChallengeId::ALL {
            let mine: Vec<&ChallengeUnlock> =
                unlocks.iter().filter(|u| u.challenge == id).collect();
            let active = engine.challenges().iter().find(|c| c.id == id);
            sticker_card(ui, id, &mine, active);
        }
    });
}

fn sticker_card(
    ui: &mut egui::Ui,
    id: ChallengeId,
    won: &[&ChallengeUnlock],
    active: Option<&ChallengeProgress>,
) {
    let p = theme::palette(ui.ctx());
    let first = won.iter().min_by_key(|u| (u.unlocked_on, u.id));
    egui::Frame::new()
        .fill(if first.is_some() {
            p.surface
        } else {
            p.sidebar
        })
        .stroke(egui::Stroke::new(1.2, p.ink.gamma_multiply(0.85)))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            // Top-down inside the wrapping row, so the text wraps.
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(SHOWCASE_CARD - 24.0);
                ui.set_min_height(340.0);
                sticker_card_body(ui, id, won, first, active);
            });
        });
}

fn sticker_card_body(
    ui: &mut egui::Ui,
    id: ChallengeId,
    won: &[&ChallengeUnlock],
    first: Option<&&ChallengeUnlock>,
    active: Option<&ChallengeProgress>,
) {
    let p = theme::palette(ui.ctx());
    ui.vertical_centered(|ui| {
        ui.add(stickers::image(ui.ctx(), id, 104.0, first.is_some()));
        ui.add_space(4.0);
        ui.label(
            RichText::new(i18n::challenge_name(id))
                .family(theme::heading_family())
                .size(16.0),
        );
    });
    let Some(first) = first else {
        widgets::chip(ui, &t!("challenges.locked"), p.muted);
        ui.label(
            RichText::new(i18n::challenge_goal(id))
                .small()
                .color(p.muted),
        );
        if let Some(progress) = active {
            ui.add_space(4.0);
            ui.label(
                RichText::new(t!(
                    "challenges.in_this_plan",
                    current = progress.current,
                    target = progress.target
                ))
                .small()
                .color(p.muted),
            );
            widgets::progress_line(ui, SHOWCASE_CARD - 40.0, progress.fraction(), "", p.accent);
        }
        return;
    };
    ui.label(RichText::new(i18n::challenge_caption(id)).small());
    ui.add_space(4.0);
    ui.label(RichText::new(won_on(first)).small().color(p.success));
    let plans: Vec<String> = won
        .iter()
        .map(|u| match u.plan {
            Some(_) => u.plan_title.clone(),
            None => t!("challenges.plan_deleted", title = u.plan_title).into_owned(),
        })
        .collect();
    ui.label(
        RichText::new(t!("challenges.won_in", plans = plans.join(", ")))
            .small()
            .color(p.muted),
    );
}

/// The discreet line on Today: the next challenge and how far it is.
pub fn today_card(ui: &mut egui::Ui, engine: &Engine, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    let Some(next) = engine.next_challenge() else {
        if !engine.challenges().is_empty() {
            ui.horizontal(|ui| {
                widgets::hint(ui, &t!("challenges.all_done"));
                if ui.link(t!("challenges.see_all")).clicked() {
                    actions.go_to(Screen::Challenges);
                }
            });
        }
        return;
    };
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, p.border))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add(stickers::image(ui.ctx(), next.id, 40.0, false));
                ui.vertical(|ui| {
                    widgets::kicker(ui, &t!("challenges.next"), p.muted);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(i18n::challenge_name(next.id)).strong());
                        ui.label(
                            RichText::new(t!(
                                "challenges.progress",
                                current = next.current,
                                target = next.target
                            ))
                            .color(p.accent),
                        );
                        ui.label(
                            RichText::new(i18n::challenge_goal(next.id))
                                .small()
                                .color(p.muted),
                        );
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t!("challenges.see_all")).clicked() {
                        actions.go_to(Screen::Challenges);
                    }
                });
            });
        });
}
