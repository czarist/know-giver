//! Challenges and the sticker showcase. Everything shown comes from the
//! engine; this screen only draws it and collects clicks.
//!
//! The challenges are shown by family (lessons, quizzes, hours…): one line
//! each with the next level, its progress and where to go. The showcase has
//! every sticker, in colour when won and grey until then.

use egui::RichText;
use kg_engine::{
    CATALOG, ChallengeId, ChallengeProgress, ChallengeUnlock, Command, Engine, Family, Group,
    NextStep, Scope, StudyPlan,
};
use rust_i18n::t;

use super::Actions;
use crate::app::{KnowGiverApp, Screen};
use crate::{i18n, stickers, theme, widgets};

/// Width of a showcase card.
const SHOWCASE_CARD: f32 = 138.0;

/// Which stickers the showcase lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Filter {
    #[default]
    All,
    Won,
    Locked,
}

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
        ui.add_space(18.0);
        overall_challenges(ui, engine);
        ui.add_space(22.0);
        showcase(ui, engine);
    });
    actions.run(app);
}

/// The challenges of one family, lowest target first.
fn family_progress<'a>(
    list: &[&'a ChallengeProgress],
    family: Family,
) -> Vec<&'a ChallengeProgress> {
    list.iter()
        .copied()
        .filter(|c| c.id.family() == family)
        .collect()
}

fn plan_challenges(ui: &mut egui::Ui, engine: &Engine, plan: &StudyPlan, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    let list = engine.challenges();
    let won = list.iter().filter(|c| c.is_met()).count();
    widgets::kicker(ui, &t!("challenges.plan_section"), p.muted);
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(&plan.title)
                .family(theme::heading_family())
                .size(18.0),
        );
        ui.label(
            RichText::new(t!("challenges.plan_summary", won = won, total = list.len()))
                .color(p.muted),
        );
    });
    ui.add_space(6.0);
    for group in Group::ALL {
        let families: Vec<Family> = Family::ALL
            .into_iter()
            .filter(|f| f.scope() == Scope::Plan && f.group() == group)
            .filter(|f| list.iter().any(|c| c.id.family() == *f))
            .collect();
        if families.is_empty() {
            continue;
        }
        group_heading(ui, group);
        for family in families {
            family_row(
                ui,
                engine,
                Some(plan),
                &family_progress(&list, family),
                actions,
            );
        }
    }
}

fn overall_challenges(ui: &mut egui::Ui, engine: &Engine) {
    let p = theme::palette(ui.ctx());
    let list: Vec<&ChallengeProgress> = engine.overall_challenges().iter().collect();
    widgets::kicker(ui, &t!("challenges.overall_section"), p.muted);
    widgets::hint(ui, &t!("challenges.overall_note"));
    ui.add_space(4.0);
    let mut none = Actions::default();
    for group in [Group::Time, Group::Collection] {
        group_heading(ui, group);
        for family in Family::ALL
            .into_iter()
            .filter(|f| f.scope() == Scope::Overall && f.group() == group)
        {
            family_row(ui, engine, None, &family_progress(&list, family), &mut none);
        }
    }
}

fn group_heading(ui: &mut egui::Ui, group: Group) {
    ui.add_space(8.0);
    ui.label(
        RichText::new(i18n::challenge_group(group))
            .family(theme::heading_family())
            .size(16.0),
    );
    ui.add_space(2.0);
}

/// One line per family: the next level (or the last, when all are won),
/// its progress, the levels won and a button to go on.
fn family_row(
    ui: &mut egui::Ui,
    engine: &Engine,
    plan: Option<&StudyPlan>,
    levels: &[&ChallengeProgress],
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    let Some(last) = levels.last() else {
        return;
    };
    let won = levels.iter().filter(|c| c.is_met()).count();
    let next = levels.iter().find(|c| !c.is_met()).copied();
    let shown = next.unwrap_or(last);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(egui::Stroke::new(1.0, p.border))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .outer_margin(egui::Margin {
            left: 0,
            right: 4,
            top: 0,
            bottom: 4,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // One line: sticker, name and goal on the left; progress and
            // the button on the right.
            ui.horizontal(|ui| {
                ui.add(stickers::image(ui.ctx(), shown.id, 40.0, next.is_none()))
                    .on_hover_text(i18n::challenge_caption(shown.id));
                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - 330.0).max(200.0));
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(i18n::challenge_name(shown.id)).strong());
                        if levels.len() > 1 {
                            widgets::chip(
                                ui,
                                &t!(
                                    "challenges.level",
                                    level = shown.id.level(),
                                    total = levels.len()
                                ),
                                p.muted,
                            );
                        }
                        if next.is_none() {
                            widgets::chip(ui, &t!("challenges.all_levels"), p.success);
                        }
                    });
                    let goal = ui.label(
                        RichText::new(i18n::challenge_goal(shown.id))
                            .small()
                            .color(p.muted),
                    );
                    // Older right answers to mistakes were not recorded.
                    if next.is_some_and(|n| n.id.family() == Family::Recoveries) && won == 0 {
                        goal.on_hover_text(t!("challenges.comeback_note"));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let Some(next) = next else {
                        if let Some(unlock) = unlock_for(engine, shown.id, plan) {
                            ui.label(RichText::new(won_on(unlock)).small().color(p.success));
                        }
                        return;
                    };
                    if !next.reachable {
                        widgets::hint(ui, &t!("challenges.out_of_reach"));
                        return;
                    }
                    if let Some(plan) = plan {
                        next_step(ui, engine, plan, next.next, actions);
                    }
                    let mut count = t!(
                        "challenges.progress",
                        current = next.current,
                        target = next.target
                    )
                    .into_owned();
                    if let Some(week) = next.week {
                        count = format!("{count} · {}", t!("challenges.week", n = week));
                    }
                    widgets::progress_line(ui, 120.0, next.fraction(), &count, p.accent);
                });
            });
        });
}

/// The won unlock of `id` in `plan` (or overall).
fn unlock_for<'a>(
    engine: &'a Engine,
    id: ChallengeId,
    plan: Option<&StudyPlan>,
) -> Option<&'a ChallengeUnlock> {
    engine
        .sticker_unlocks()
        .iter()
        .find(|u| u.challenge == id && (plan.is_none() || u.plan == plan.map(|p| p.id)))
}

/// Where to go next, as a button.
fn next_step(
    ui: &mut egui::Ui,
    engine: &Engine,
    plan: &StudyPlan,
    next: NextStep,
    actions: &mut Actions,
) {
    let topic = |id| plan.find_topic(id);
    let (label, command, screen, place) = match next {
        // Always a lesson that counts: continue one left halfway or start
        // one, never the recap of a lesson already taken.
        NextStep::Lesson(id) if engine.has_lesson_draft(id) => (
            t!("lesson.continue"),
            Some(Command::ResumeLesson(id)),
            Screen::Lesson,
            topic(id),
        ),
        NextStep::Lesson(id) => (
            t!("lesson.start"),
            Some(Command::StartLesson(id)),
            Screen::Lesson,
            topic(id),
        ),
        NextStep::Quiz(id) => (
            t!("challenges.open_quiz"),
            Some(Command::StartQuiz(id)),
            Screen::Quiz,
            topic(id),
        ),
        NextStep::Mistakes(id) => (
            t!("challenges.open_mistakes"),
            Some(Command::RetryMistakes(id)),
            Screen::Quiz,
            topic(id),
        ),
        NextStep::Review(id) => (
            t!("challenges.open_review"),
            Some(Command::StartReview(id)),
            Screen::Review,
            topic(id),
        ),
        NextStep::Board => (t!("challenges.open_board"), None, Screen::Board, None),
        NextStep::Reader => (t!("challenges.open_reader"), None, Screen::Reader, None),
        NextStep::Writing => (
            t!("challenges.open_writing"),
            None,
            Screen::Discursive,
            None,
        ),
        NextStep::None => return,
    };
    let mut button = widgets::primary_button(ui, &label);
    // The topic it opens, on hover.
    if let Some((subject, topic)) = place {
        button = button.on_hover_text(t!(
            "challenges.topic",
            topic = topic.title,
            subject = subject.name
        ));
    }
    if button.clicked() {
        if let Some(command) = command {
            actions.send(command);
        }
        actions.go_to(screen);
    }
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

/// Every sticker, Steam-style: colour when won anywhere, grey with the
/// requirement until then; grouped like the challenges.
fn showcase(ui: &mut egui::Ui, engine: &Engine) {
    let p = theme::palette(ui.ctx());
    let unlocks = engine.sticker_unlocks();
    let won = CATALOG
        .iter()
        .filter(|c| unlocks.iter().any(|u| u.challenge.key() == c.key))
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
                total = CATALOG.len()
            ))
            .color(p.muted),
        );
    });
    widgets::hint(ui, &t!("challenges.showcase_note"));
    let filter_id = egui::Id::new("showcase_filter");
    let mut filter: Filter = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
    ui.horizontal(|ui| {
        for (value, label) in [
            (Filter::All, t!("challenges.filter_all")),
            (Filter::Won, t!("challenges.filter_won")),
            (Filter::Locked, t!("challenges.filter_locked")),
        ] {
            if ui.selectable_label(filter == value, label).clicked() {
                filter = value;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(filter_id, filter));
    let active: Vec<&ChallengeProgress> = engine
        .challenges()
        .into_iter()
        .chain(engine.overall_challenges())
        .collect();
    for group in Group::ALL {
        let ids: Vec<ChallengeId> = ChallengeId::all()
            .filter(|id| id.group() == group)
            .filter(|id| {
                let has = unlocks.iter().any(|u| u.challenge == *id);
                match filter {
                    Filter::All => true,
                    Filter::Won => has,
                    Filter::Locked => !has,
                }
            })
            .collect();
        if ids.is_empty() {
            continue;
        }
        group_heading(ui, group);
        // Rows of as many cards as fit the width.
        let gap = 10.0;
        let columns = (((ui.available_width() - 4.0) + gap) / (SHOWCASE_CARD + 2.0 + gap))
            .floor()
            .max(1.0) as usize;
        for row in ids.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                for &id in row {
                    let mine: Vec<&ChallengeUnlock> =
                        unlocks.iter().filter(|u| u.challenge == id).collect();
                    let progress = active.iter().find(|c| c.id == id).copied();
                    sticker_card(ui, id, &mine, progress);
                }
            });
            ui.add_space(gap);
        }
    }
}

fn sticker_card(
    ui: &mut egui::Ui,
    id: ChallengeId,
    won: &[&ChallengeUnlock],
    active: Option<&ChallengeProgress>,
) {
    let p = theme::palette(ui.ctx());
    let first = won.iter().min_by_key(|u| (u.unlocked_on, u.id)).copied();
    let response = egui::Frame::new()
        .fill(if first.is_some() {
            p.surface
        } else {
            p.sidebar
        })
        .stroke(egui::Stroke::new(1.0, p.ink.gamma_multiply(0.6)))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            let size = egui::vec2(SHOWCASE_CARD - 16.0, 150.0);
            ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.set_width(size.x);
                ui.set_min_height(size.y);
                ui.add(stickers::image(ui.ctx(), id, 76.0, first.is_some()));
                ui.add(
                    egui::Label::new(
                        RichText::new(i18n::challenge_name(id))
                            .small()
                            .strong()
                            .color(if first.is_some() { p.text } else { p.muted }),
                    )
                    .wrap(),
                );
                match (first, active) {
                    (Some(first), _) => {
                        ui.label(RichText::new(won_on(first)).small().color(p.success));
                    }
                    (None, Some(progress)) if progress.reachable => {
                        widgets::progress_line(
                            ui,
                            SHOWCASE_CARD - 60.0,
                            progress.fraction(),
                            &t!(
                                "challenges.progress",
                                current = progress.current,
                                target = progress.target
                            ),
                            p.accent,
                        );
                    }
                    _ => widgets::chip(ui, &t!("challenges.locked"), p.muted),
                }
            });
        })
        .response;
    // The details on hover: what it asks, its story and where it was won.
    response.on_hover_ui(|ui| {
        ui.set_max_width(280.0);
        ui.label(RichText::new(i18n::challenge_name(id)).strong());
        ui.label(i18n::challenge_goal(id));
        if let Some(first) = first {
            ui.label(RichText::new(i18n::challenge_caption(id)).italics());
            ui.label(RichText::new(won_on(first)).small());
            let plans: Vec<String> = won
                .iter()
                .map(|u| match u.plan {
                    Some(_) => u.plan_title.clone(),
                    None if id.scope() == Scope::Overall => {
                        t!("challenges.overall_won").into_owned()
                    }
                    None => t!("challenges.plan_deleted", title = u.plan_title).into_owned(),
                })
                .collect();
            ui.label(RichText::new(t!("challenges.won_in", plans = plans.join(", "))).small());
        }
    });
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
