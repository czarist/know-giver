//! Written answers ("Discursivas"): ask for a proposal, photograph the
//! handwritten pages, check the transcript, read the formative grade and
//! follow progress. Shown only when the plan's notice has a written test.

use egui::{RichText, vec2};
use kg_engine::{
    Attempt, AttemptId, AttemptOrigin, AttemptStatus, Command, Difficulty, Engine, Evaluation,
    Evidence, OpenAttempt, Page, PageId, Proposal, ProposalId, ProposalRequest, RubricSource,
    StudyPlan, TopicId, VisionCheck, WritingKind,
};
use rust_i18n::t;

use super::Actions;
use crate::app::{KnowGiverApp, Screen};
use crate::{i18n, theme, widgets};

/// Height of a page thumbnail.
const THUMB_HEIGHT: f32 = 200.0;

/// Form contents and view choices of this screen.
#[derive(Debug, Default)]
pub struct DiscursiveUi {
    pub kind: Option<WritingKind>,
    pub difficulty: Difficulty,
    /// Pick topics by hand instead of a suggestion.
    pub pick_topics: bool,
    pub topics: Vec<TopicId>,
    /// Criteria typed by the student.
    pub manual: bool,
    pub manual_text: String,
    pub line_limit: Option<u32>,
    pub minutes: Option<u32>,
    pub history: bool,
    /// Transcript being checked: attempt and one text per page.
    pub editing: Option<(AttemptId, Vec<String>)>,
    pub review_page: usize,
    pub zoom: Option<PageId>,
    pub confirm_delete_attempt: Option<AttemptId>,
    pub confirm_delete_proposal: Option<ProposalId>,
    pub show_guide: Option<ProposalId>,
}

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    let state = &mut app.ui.discursive;
    widgets::header(ui, &t!("discursive.title"), &t!("discursive.subtitle"));

    let Some(plan) = engine.active_plan() else {
        widgets::hint(ui, &t!("plan.empty"));
        if ui.button(t!("plan.go_to_edital")).clicked() {
            actions.go_to(Screen::Edital);
        }
        actions.run(app);
        return;
    };
    if !engine.writing_rules().has_written_test() {
        let revalidating = engine.revalidating().is_some();
        let label = t!("edital.revalidate");
        if widgets::empty_state(
            ui,
            crate::icons::Icon::Edital,
            &t!("discursive.no_test_title"),
            &t!("discursive.no_test"),
            (!revalidating).then_some(label.as_ref()),
        ) && let Some(path) = rfd::FileDialog::new()
            .set_title(t!("edital.revalidate"))
            .add_filter(t!("edital.pdf_filter"), &["pdf"])
            .pick_file()
        {
            actions.send(Command::RevalidateEdital {
                plan: plan.id,
                path,
            });
        }
        if revalidating {
            ui.vertical_centered(|ui| ui.spinner());
        }
        widgets::hint(ui, &t!("edital.revalidate_hint"));
        actions.run(app);
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        if let Some(open) = engine.open_attempt() {
            attempt_view(ui, engine, plan, open, state, &mut actions);
        } else {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.history, false, t!("discursive.tab_proposals"));
                ui.selectable_value(&mut state.history, true, t!("discursive.tab_history"));
            });
            ui.add_space(8.0);
            if state.history {
                history(ui, engine, &mut actions);
            } else {
                vision_card(ui, engine, &mut actions);
                ui.add_space(12.0);
                new_proposal(ui, engine, plan, state, &mut actions);
                ui.add_space(12.0);
                proposals(ui, engine, plan, state, &mut actions);
            }
        }
    });
    actions.run(app);
}

/// Where photos go and whether the vision model reads them.
fn vision_card(ui: &mut egui::Ui, engine: &Engine, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    match engine.vision_target() {
        None => widgets::callout(ui, p.warning, |ui| {
            ui.label(RichText::new(t!("discursive.vision_missing")).strong());
            widgets::hint(ui, &t!("discursive.vision_howto"));
            if ui.button(t!("discursive.open_settings")).clicked() {
                actions.go_to(Screen::Settings);
            }
        }),
        Some((model, host)) => widgets::callout(ui, p.accent, |ui| {
            ui.label(t!("discursive.vision_privacy", model = model, host = host));
            ui.horizontal(|ui| {
                let checking = engine.vision_check() == VisionCheck::Checking;
                if ui
                    .add_enabled(!checking, egui::Button::new(t!("discursive.vision_test")))
                    .on_hover_text(t!("discursive.vision_test_hint"))
                    .clicked()
                {
                    actions.send(Command::CheckVision);
                }
                match engine.vision_check() {
                    VisionCheck::Checking => {
                        ui.spinner();
                    }
                    VisionCheck::Works => {
                        ui.colored_label(p.success, t!("discursive.vision_works"));
                    }
                    VisionCheck::NotReading => {
                        ui.colored_label(p.danger, t!("discursive.vision_not_reading"));
                    }
                    VisionCheck::Unknown => {}
                }
            });
        }),
    }
}

fn new_proposal(
    ui: &mut egui::Ui,
    engine: &Engine,
    plan: &StudyPlan,
    state: &mut DiscursiveUi,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    let kinds = engine.writing_rules().kinds.clone();
    if state.kind.is_none_or(|k| !kinds.contains(&k)) {
        state.kind = kinds.first().copied();
    }
    widgets::card(ui, |ui| {
        ui.label(
            RichText::new(t!("discursive.new_title"))
                .family(theme::heading_family())
                .size(18.0),
        );
        let names: Vec<String> = kinds.iter().map(|k| i18n::writing_kind(*k)).collect();
        widgets::hint(ui, &t!("discursive.notice_has", kinds = names.join(", ")));
        ui.add_space(6.0);
        egui::Grid::new("writing_form")
            .num_columns(2)
            .spacing(vec2(12.0, 8.0))
            .show(ui, |ui| {
                ui.label(t!("discursive.kind_label"));
                ui.horizontal_wrapped(|ui| {
                    for kind in &kinds {
                        ui.radio_value(&mut state.kind, Some(*kind), i18n::writing_kind(*kind));
                    }
                });
                ui.end_row();
                ui.label(t!("discursive.difficulty_label"));
                ui.horizontal(|ui| {
                    for d in Difficulty::ALL {
                        ui.radio_value(&mut state.difficulty, d, i18n::difficulty(d));
                    }
                });
                ui.end_row();
                ui.label(t!("discursive.topics_label"));
                ui.vertical(|ui| {
                    ui.radio_value(
                        &mut state.pick_topics,
                        false,
                        t!("discursive.topics_suggest"),
                    );
                    ui.radio_value(&mut state.pick_topics, true, t!("discursive.topics_pick"));
                });
                ui.end_row();
            });
        if state.pick_topics {
            egui::ScrollArea::vertical()
                .id_salt("writing_topics")
                .max_height(180.0)
                .show(ui, |ui| {
                    for subject in &plan.subjects {
                        ui.label(RichText::new(&subject.name).strong());
                        for topic in &subject.topics {
                            let mut on = state.topics.contains(&topic.id);
                            if ui.checkbox(&mut on, &topic.title).changed() {
                                if on {
                                    state.topics.push(topic.id);
                                } else {
                                    state.topics.retain(|t| *t != topic.id);
                                }
                            }
                        }
                    }
                });
        }
        ui.add_space(6.0);
        ui.label(t!("discursive.criteria_label"));
        ui.radio_value(&mut state.manual, false, t!("discursive.criteria_auto"));
        ui.radio_value(&mut state.manual, true, t!("discursive.criteria_manual"));
        if state.manual {
            ui.add(
                egui::TextEdit::multiline(&mut state.manual_text)
                    .desired_rows(4)
                    .hint_text(t!("discursive.criteria_manual_hint")),
            );
        }
        ui.horizontal(|ui| {
            optional_number(ui, &mut state.line_limit, &t!("discursive.line_limit"), 30);
            optional_number(ui, &mut state.minutes, &t!("discursive.minutes"), 60);
        });
        widgets::hint(ui, &t!("discursive.limits_hint"));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let busy = engine.is_generating_proposal();
            let ready = state.kind.is_some()
                && (!state.pick_topics || !state.topics.is_empty())
                && (!state.manual || !state.manual_text.trim().is_empty());
            if ui
                .add_enabled(
                    ready && !busy,
                    egui::Button::new(RichText::new(t!("discursive.generate")).color(p.surface))
                        .fill(p.accent),
                )
                .clicked()
                && let Some(kind) = state.kind
            {
                actions.send(Command::GenerateProposal(ProposalRequest {
                    kind,
                    difficulty: state.difficulty,
                    topics: if state.pick_topics {
                        state.topics.clone()
                    } else {
                        Vec::new()
                    },
                    manual_rubric: state.manual.then(|| state.manual_text.clone()),
                    line_limit: state.line_limit,
                    minutes: state.minutes,
                }));
            }
            if busy {
                ui.spinner();
                widgets::hint(ui, &t!("discursive.generating"));
            }
        });
    });
}

fn optional_number(ui: &mut egui::Ui, value: &mut Option<u32>, label: &str, default: u32) {
    let mut on = value.is_some();
    if ui.checkbox(&mut on, label).changed() {
        *value = on.then_some(default);
    }
    if let Some(v) = value {
        ui.add(egui::DragValue::new(v).range(1..=600));
    }
}

fn proposals(
    ui: &mut egui::Ui,
    engine: &Engine,
    plan: &StudyPlan,
    state: &mut DiscursiveUi,
    actions: &mut Actions,
) {
    if engine.proposals().is_empty() {
        widgets::hint(ui, &t!("discursive.no_proposals"));
        return;
    }
    let p = theme::palette(ui.ctx());
    for proposal in engine.proposals() {
        widgets::card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&proposal.theme).strong().size(16.0));
                widgets::chip(ui, &i18n::writing_kind(proposal.kind), p.accent);
                widgets::chip(ui, &i18n::difficulty(proposal.difficulty), p.muted);
                widgets::chip(ui, &i18n::rubric_source(proposal.rubric.source), p.muted);
            });
            widgets::hint(ui, &topic_names(plan, &proposal.topics));
            let attempts: Vec<&Attempt> = engine
                .writing_attempts()
                .iter()
                .filter(|a| a.proposal_id == proposal.id)
                .collect();
            for attempt in &attempts {
                ui.horizontal(|ui| {
                    ui.label(t!(
                        "discursive.attempt_row",
                        number = attempt.number,
                        date = i18n::date(attempt.created_at.date())
                    ));
                    widgets::chip(
                        ui,
                        &i18n::attempt_status(attempt.status),
                        status_color(ui, attempt.status),
                    );
                    if let Some((total, max)) = attempt.score {
                        ui.label(t!(
                            "discursive.score_short",
                            total = i18n::points(total),
                            max = i18n::points(max)
                        ));
                    }
                    if ui.small_button(t!("discursive.open")).clicked() {
                        actions.send(Command::OpenAttempt(attempt.id));
                    }
                });
            }
            ui.horizontal(|ui| {
                let label = if attempts.is_empty() {
                    t!("discursive.start")
                } else {
                    t!("discursive.new_attempt")
                };
                if widgets::primary_button(ui, &label).clicked() {
                    actions.send(Command::StartAttempt(proposal.id));
                }
                if ui.button(t!("edital.delete")).clicked() {
                    state.confirm_delete_proposal = Some(proposal.id);
                }
            });
            if state.confirm_delete_proposal == Some(proposal.id) {
                widgets::callout(ui, p.danger, |ui| {
                    ui.label(t!("discursive.confirm_delete_proposal"));
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new(t!("edital.delete")).color(p.danger))
                            .clicked()
                        {
                            actions.send(Command::DeleteProposal(proposal.id));
                            state.confirm_delete_proposal = None;
                        }
                        if ui.button(t!("common.cancel")).clicked() {
                            state.confirm_delete_proposal = None;
                        }
                    });
                });
            }
        });
        ui.add_space(8.0);
    }
}

fn topic_names(plan: &StudyPlan, topics: &[TopicId]) -> String {
    topics
        .iter()
        .filter_map(|id| plan.find_topic(*id))
        .map(|(_, t)| t.title.clone())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn status_color(ui: &egui::Ui, status: AttemptStatus) -> egui::Color32 {
    let p = theme::palette(ui.ctx());
    match status {
        AttemptStatus::Evaluated => p.success,
        AttemptStatus::Failed | AttemptStatus::Retake => p.danger,
        AttemptStatus::Pending | AttemptStatus::Review => p.warning,
        _ => p.muted,
    }
}

/// The proposal, its criteria and limits, shown before and during attempts.
fn proposal_card(ui: &mut egui::Ui, plan: &StudyPlan, proposal: &Proposal) {
    let p = theme::palette(ui.ctx());
    widgets::card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            widgets::chip(ui, &i18n::writing_kind(proposal.kind), p.accent);
            widgets::chip(ui, &i18n::difficulty(proposal.difficulty), p.muted);
            if let Some(lines) = proposal.line_limit {
                widgets::chip(ui, &t!("discursive.lines_chip", lines = lines), p.muted);
            }
            if let Some(minutes) = proposal.minutes {
                widgets::chip(
                    ui,
                    &t!("discursive.minutes_chip", minutes = minutes),
                    p.muted,
                );
            }
        });
        ui.label(RichText::new(&proposal.theme).strong().size(17.0));
        ui.label(&proposal.statement);
        if !proposal.instructions.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new(&proposal.instructions).italics());
        }
        widgets::hint(
            ui,
            &t!(
                "discursive.topics_used",
                topics = topic_names(plan, &proposal.topics)
            ),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new(t!(
                "discursive.criteria_title",
                max = i18n::points(proposal.rubric.max_total())
            ))
            .strong(),
        );
        for c in &proposal.rubric.criteria {
            ui.horizontal_wrapped(|ui| {
                ui.label(i18n::criterion_name(c));
                widgets::chip(
                    ui,
                    &t!("discursive.points", points = i18n::points(c.max_points)),
                    p.accent,
                );
            });
            let description = i18n::criterion_description(c);
            if !description.is_empty() {
                ui.label(RichText::new(description).small().color(p.muted));
            }
            ui.add_space(2.0);
        }
        let source = match proposal.rubric.source {
            RubricSource::Edital => t!("discursive.source_edital"),
            RubricSource::Manual => t!("discursive.source_manual"),
            RubricSource::Generic => t!("discursive.source_generic"),
        };
        widgets::hint(ui, &source);
        if proposal.rubric.source == RubricSource::Edital {
            egui::CollapsingHeader::new(t!("discursive.source_passage"))
                .id_salt(("passage", proposal.id.0))
                .show(ui, |ui| {
                    ui.label(RichText::new(&proposal.rubric.evidence).italics());
                });
        }
    });
}

fn attempt_view(
    ui: &mut egui::Ui,
    engine: &Engine,
    plan: &StudyPlan,
    open: &OpenAttempt,
    state: &mut DiscursiveUi,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    let attempt = &open.attempt;
    ui.horizontal(|ui| {
        if ui.button(t!("common.back")).clicked() {
            actions.send(Command::CloseAttempt);
            state.editing = None;
            state.zoom = None;
        }
        ui.label(RichText::new(t!("discursive.attempt_title", number = attempt.number)).strong());
        widgets::chip(
            ui,
            &i18n::attempt_status(attempt.status),
            status_color(ui, attempt.status),
        );
        if attempt.origin == AttemptOrigin::Rewrite {
            widgets::chip(ui, &t!("discursive.rewrite_chip"), p.muted);
        }
    });
    ui.add_space(8.0);
    proposal_card(ui, plan, &open.proposal);
    ui.add_space(12.0);

    match attempt.status {
        AttemptStatus::Draft | AttemptStatus::Retake => {
            if attempt.status == AttemptStatus::Retake {
                widgets::callout(ui, p.danger, |ui| {
                    ui.label(RichText::new(t!("discursive.retake_title")).strong());
                    for line in attempt.note.lines() {
                        ui.label(t!("discursive.retake_page", reason = line));
                    }
                    widgets::hint(ui, &t!("discursive.retake_hint"));
                });
                ui.add_space(8.0);
            }
            pages_editor(ui, engine, open, state, actions);
        }
        AttemptStatus::Transcribing => working(ui, &t!("discursive.transcribing")),
        AttemptStatus::Evaluating => working(ui, &t!("discursive.evaluating")),
        AttemptStatus::Review | AttemptStatus::Pending => {
            if attempt.status == AttemptStatus::Pending {
                widgets::callout(ui, p.warning, |ui| {
                    ui.label(RichText::new(t!("discursive.pending_title")).strong());
                    if attempt.note == kg_engine::NOTE_UNREADABLE {
                        ui.label(t!("discursive.pending_unreadable"));
                    } else {
                        ui.label(&attempt.note);
                    }
                    widgets::hint(ui, &t!("discursive.pending_hint"));
                });
                ui.add_space(8.0);
            }
            review_panel(ui, engine, open, state, actions);
        }
        AttemptStatus::Failed => {
            widgets::callout(ui, p.danger, |ui| {
                let stage = match attempt.failed_stage {
                    Some(kg_engine::Stage::Evaluation) => t!("discursive.failed_evaluation"),
                    _ => t!("discursive.failed_transcription"),
                };
                ui.label(RichText::new(stage).strong());
                if attempt.note == kg_engine::NOTE_INTERRUPTED {
                    ui.label(t!("discursive.failed_interrupted"));
                }
                if widgets::primary_button(ui, &t!("discursive.retry")).clicked() {
                    actions.send(Command::RetryAttempt);
                }
            });
        }
        AttemptStatus::Evaluated => {
            if let Some(eval) = &open.evaluation {
                evaluation_view(ui, engine, plan, open, eval, state, actions);
            }
        }
    }

    ui.add_space(16.0);
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button(t!("discursive.delete_attempt")).clicked() {
            state.confirm_delete_attempt = Some(attempt.id);
        }
        if state.confirm_delete_attempt == Some(attempt.id) {
            ui.label(RichText::new(t!("discursive.confirm_delete_attempt")).color(p.danger));
            if ui
                .button(RichText::new(t!("edital.delete")).color(p.danger))
                .clicked()
            {
                actions.send(Command::DeleteAttempt(attempt.id));
                state.confirm_delete_attempt = None;
                state.editing = None;
            }
            if ui.button(t!("common.cancel")).clicked() {
                state.confirm_delete_attempt = None;
            }
        }
    });
}

fn working(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(text);
    });
    widgets::hint(ui, &t!("discursive.working_hint"));
}

/// A page's photo, or a spinner while it loads.
fn page_photo(
    ui: &mut egui::Ui,
    engine: &Engine,
    page: &Page,
    max: egui::Vec2,
) -> Option<egui::Response> {
    match engine.page_image(page.id) {
        Some((version, bytes)) => Some(
            ui.add(
                egui::Image::from_bytes(
                    format!("bytes://kg-page-{}-{version}.jpg", page.id.0),
                    egui::load::Bytes::Shared(bytes),
                )
                .max_size(max)
                .sense(egui::Sense::click()),
            ),
        ),
        None => {
            ui.spinner();
            None
        }
    }
}

fn pages_editor(
    ui: &mut egui::Ui,
    engine: &Engine,
    open: &OpenAttempt,
    state: &mut DiscursiveUi,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    ui.label(
        RichText::new(t!("discursive.pages_title", max = kg_engine::MAX_PAGES))
            .family(theme::heading_family())
            .size(18.0),
    );
    widgets::hint(ui, &t!("discursive.pages_hint"));
    ui.add_space(6.0);

    if let Some(zoom) = state.zoom
        && let Some(page) = open.pages.iter().find(|p| p.id == zoom)
    {
        widgets::card(ui, |ui| {
            if ui.button(t!("discursive.zoom_close")).clicked() {
                state.zoom = None;
            }
            let width = ui.available_width();
            page_photo(ui, engine, page, vec2(width, width * 1.5));
        });
        ui.add_space(8.0);
    }

    let count = open.pages.len();
    ui.horizontal_wrapped(|ui| {
        for (i, page) in open.pages.iter().enumerate() {
            widgets::card(ui, |ui| {
                ui.set_width(170.0);
                ui.label(RichText::new(t!("discursive.page_n", n = i + 1)).strong());
                if page_photo(ui, engine, page, vec2(160.0, THUMB_HEIGHT))
                    .is_some_and(|r| r.on_hover_text(t!("discursive.zoom")).clicked())
                {
                    state.zoom = Some(page.id);
                }
                for issue in &page.issues {
                    let color = if issue.blocks() { p.danger } else { p.warning };
                    widgets::chip(ui, &i18n::page_issue(*issue), color);
                }
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .small_button("⟳")
                        .on_hover_text(t!("discursive.rotate"))
                        .clicked()
                    {
                        actions.send(Command::RotatePage(page.id));
                    }
                    if ui
                        .add_enabled(i > 0, egui::Button::new("↑").small())
                        .on_hover_text(t!("discursive.move_up"))
                        .clicked()
                    {
                        actions.send(Command::MovePage {
                            page: page.id,
                            up: true,
                        });
                    }
                    if ui
                        .add_enabled(i + 1 < count, egui::Button::new("↓").small())
                        .on_hover_text(t!("discursive.move_down"))
                        .clicked()
                    {
                        actions.send(Command::MovePage {
                            page: page.id,
                            up: false,
                        });
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button(t!("discursive.replace")).clicked()
                        && let Some(path) = pick_photos(false).into_iter().next()
                    {
                        actions.send(Command::ReplacePage {
                            page: page.id,
                            path,
                        });
                    }
                    if ui.small_button(t!("discursive.remove")).clicked() {
                        actions.send(Command::RemovePage(page.id));
                        if state.zoom == Some(page.id) {
                            state.zoom = None;
                        }
                    }
                });
            });
        }
    });
    if engine.pages_in_progress() > 0 {
        ui.horizontal(|ui| {
            ui.spinner();
            widgets::hint(ui, &t!("discursive.pages_loading"));
        });
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                count < kg_engine::MAX_PAGES,
                egui::Button::new(t!("discursive.add_pages")),
            )
            .clicked()
        {
            let paths = pick_photos(true);
            if !paths.is_empty() {
                actions.send(Command::AddPages(paths));
            }
        }
        let blocked = open
            .pages
            .iter()
            .any(|p| p.issues.iter().any(|i| i.blocks()));
        let ready = count > 0
            && !blocked
            && engine.pages_in_progress() == 0
            && engine.vision_target().is_some();
        if ui
            .add_enabled(
                ready,
                egui::Button::new(RichText::new(t!("discursive.send")).color(p.surface))
                    .fill(p.accent),
            )
            .clicked()
        {
            actions.send(Command::SubmitPages);
        }
    });
    match engine.vision_target() {
        Some((model, host)) => {
            widgets::hint(
                ui,
                &t!("discursive.send_privacy", model = model, host = host),
            );
        }
        None => {
            ui.colored_label(p.warning, t!("discursive.vision_missing"));
        }
    }
}

fn pick_photos(many: bool) -> Vec<std::path::PathBuf> {
    let dialog = rfd::FileDialog::new()
        .set_title(t!("discursive.pick_title"))
        .add_filter(t!("discursive.photo_filter"), &["jpg", "jpeg", "png"]);
    if many {
        dialog.pick_files().unwrap_or_default()
    } else {
        dialog.pick_file().into_iter().collect()
    }
}

/// Photo and transcript side by side, for the student to fix misreadings.
fn review_panel(
    ui: &mut egui::Ui,
    engine: &Engine,
    open: &OpenAttempt,
    state: &mut DiscursiveUi,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    let Some(record) = &open.transcript else {
        return;
    };
    let original = record.original.page_texts();
    if state
        .editing
        .as_ref()
        .is_none_or(|(id, _)| *id != open.attempt.id)
    {
        let start = record
            .confirmed
            .as_ref()
            .map(|c| c.pages.clone())
            .unwrap_or_else(|| original.clone());
        state.editing = Some((open.attempt.id, start));
        state.review_page = 0;
    }
    let Some((_, texts)) = state.editing.as_mut() else {
        return;
    };
    ui.label(
        RichText::new(t!("discursive.review_title"))
            .family(theme::heading_family())
            .size(18.0),
    );
    widgets::hint(ui, &t!("discursive.review_hint"));
    widgets::hint(ui, &t!("discursive.marks_legend"));
    ui.add_space(4.0);
    let pages = record.original.pages.len();
    state.review_page = state.review_page.min(pages.saturating_sub(1));
    if pages > 1 {
        ui.horizontal(|ui| {
            for i in 0..pages {
                ui.selectable_value(
                    &mut state.review_page,
                    i,
                    t!("discursive.page_n", n = i + 1),
                );
            }
        });
    }
    let index = state.review_page;
    let width = ui.available_width();
    let photo_width = if open.pages.is_empty() {
        0.0
    } else {
        (width * 0.48).max(240.0)
    };
    ui.horizontal_top(|ui| {
        if let Some(page) = open.pages.get(index) {
            ui.vertical(|ui| {
                ui.set_width(photo_width);
                page_photo(ui, engine, page, vec2(photo_width, photo_width * 1.45));
            });
        }
        ui.vertical(|ui| {
            if let Some(text) = texts.get_mut(index) {
                ui.add(
                    egui::TextEdit::multiline(text)
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(22)
                        .desired_width(f32::INFINITY),
                );
            }
            if let Some(page) = record.original.pages.get(index) {
                if !page.uncertain.is_empty() {
                    ui.label(RichText::new(t!("discursive.uncertain_title")).strong());
                    for u in &page.uncertain {
                        let line = u.line.map(|l| l.to_string()).unwrap_or_else(|| "?".into());
                        ui.label(
                            RichText::new(t!(
                                "discursive.uncertain_row",
                                line = line,
                                text = u.text,
                                note = u.note
                            ))
                            .small(),
                        );
                    }
                }
                for warning in &page.warnings {
                    ui.label(RichText::new(warning).small().color(p.warning));
                }
            }
        });
    });
    ui.add_space(8.0);
    let mut saved_rewrite = false;
    let ratio = kg_engine::change_ratio(&record.original, texts);
    let rewrite = ratio > kg_engine::REWRITE_RATIO;
    if rewrite {
        widgets::callout(ui, p.warning, |ui| {
            ui.label(RichText::new(t!("discursive.rewrite_title")).strong());
            widgets::hint(ui, &t!("discursive.rewrite_hint"));
        });
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !rewrite,
                egui::Button::new(RichText::new(t!("discursive.confirm")).color(p.surface))
                    .fill(p.accent),
            )
            .clicked()
        {
            actions.send(Command::ConfirmTranscript(texts.clone()));
        }
        if rewrite && ui.button(t!("discursive.save_rewrite")).clicked() {
            actions.send(Command::SaveRewrite(texts.clone()));
            saved_rewrite = true;
        }
        if ui.button(t!("discursive.undo_edits")).clicked() {
            *texts = original.clone();
        }
    });
    widgets::hint(ui, &t!("discursive.confirm_hint"));
    if saved_rewrite {
        state.editing = None;
    }
}

fn evidence_line(ui: &mut egui::Ui, evidence: &Evidence) {
    let p = theme::palette(ui.ctx());
    let place = match (evidence.page, evidence.line) {
        (Some(page), Some(line)) => t!("discursive.place", page = page, line = line).into_owned(),
        _ => String::new(),
    };
    ui.label(
        RichText::new(format!("“{}” {place}", evidence.quote))
            .italics()
            .small()
            .color(p.muted),
    );
}

fn evaluation_view(
    ui: &mut egui::Ui,
    engine: &Engine,
    plan: &StudyPlan,
    open: &OpenAttempt,
    eval: &Evaluation,
    state: &mut DiscursiveUi,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    widgets::callout(ui, p.accent, |ui| {
        ui.label(RichText::new(t!("discursive.formative")).strong());
        widgets::hint(
            ui,
            &t!(
                "discursive.graded_by",
                model = eval.model,
                version = eval.prompt_version,
                date = i18n::date(eval.evaluated_at.date())
            ),
        );
    });
    ui.add_space(8.0);
    ui.label(
        RichText::new(t!(
            "discursive.score",
            total = i18n::points(eval.total()),
            max = i18n::points(eval.max_total())
        ))
        .family(theme::heading_family())
        .size(26.0),
    );
    ui.add_space(6.0);
    for criterion in &eval.rubric.criteria {
        let Some(score) = eval.scores.iter().find(|s| s.key == criterion.key) else {
            continue;
        };
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(i18n::criterion_name(criterion)).strong());
                ui.label(t!(
                    "discursive.score_short",
                    total = i18n::points(score.score),
                    max = i18n::points(criterion.max_points)
                ));
            });
            ui.add(
                egui::ProgressBar::new((score.score / criterion.max_points) as f32)
                    .desired_height(6.0),
            );
            ui.label(&score.justification);
            for evidence in &score.evidence {
                evidence_line(ui, evidence);
            }
        });
        ui.add_space(4.0);
    }
    if eval.dropped_quotes > 0 {
        widgets::hint(
            ui,
            &t!("discursive.dropped_quotes", count = eval.dropped_quotes),
        );
    }

    if !eval.strengths.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(t!("discursive.strengths")).strong());
        for remark in &eval.strengths {
            ui.label(format!("✔ {}", remark.text));
            if let Some(e) = &remark.evidence {
                evidence_line(ui, e);
            }
        }
    }
    if !eval.problems.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(t!("discursive.problems")).strong());
        for problem in &eval.problems {
            ui.horizontal_wrapped(|ui| {
                widgets::chip(ui, &i18n::problem_category(problem.category), p.warning);
                ui.label(&problem.text);
            });
            if let Some(e) = &problem.evidence {
                evidence_line(ui, e);
            }
            if !problem.suggestion.is_empty() {
                ui.label(
                    RichText::new(t!("discursive.suggestion", text = problem.suggestion))
                        .color(p.accent),
                );
            }
            ui.add_space(4.0);
        }
    }
    if !eval.priorities.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(t!("discursive.priorities")).strong());
        for (i, priority) in eval.priorities.iter().enumerate() {
            ui.label(format!("{}. {priority}", i + 1));
        }
    }
    if !eval.review_topics.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(t!("discursive.review_topics")).strong());
        widgets::hint(ui, &t!("discursive.review_topics_hint"));
        for id in &eval.review_topics {
            if let Some((subject, topic)) = plan.find_topic(*id) {
                ui.horizontal(|ui| {
                    ui.label(format!("{} — {}", subject.name, topic.title));
                    super::lesson_button(ui, engine, topic, actions);
                });
            }
        }
    }

    ui.add_space(8.0);
    if let Some(confirmed) = open.transcript.as_ref().and_then(|t| t.confirmed.as_ref()) {
        egui::CollapsingHeader::new(t!("discursive.your_text"))
            .id_salt(("your_text", open.attempt.id.0))
            .show(ui, |ui| {
                for (i, page) in confirmed.pages.iter().enumerate() {
                    ui.label(RichText::new(t!("discursive.page_n", n = i + 1)).strong());
                    ui.label(RichText::new(page).monospace());
                }
                if !confirmed.edits.is_empty() {
                    widgets::hint(
                        ui,
                        &t!("discursive.edits_count", count = confirmed.edits.len()),
                    );
                }
            });
    }

    ui.add_space(8.0);
    let proposal = &open.proposal;
    if state.show_guide == Some(proposal.id) {
        if let Some(points) = engine.expected_points(proposal) {
            widgets::callout(ui, p.highlight, |ui| {
                ui.label(RichText::new(t!("discursive.guide_title")).strong());
                widgets::hint(ui, &t!("discursive.guide_hint"));
                for point in points {
                    ui.label(format!("• {point}"));
                }
            });
        }
    } else if ui.button(t!("discursive.show_guide")).clicked() {
        state.show_guide = Some(proposal.id);
    }
    ui.add_space(8.0);
    if widgets::primary_button(ui, &t!("discursive.new_attempt")).clicked() {
        actions.send(Command::StartAttempt(proposal.id));
    }
}

/// Scores by criterion over time, problems by kind and recurring ones.
fn history(ui: &mut egui::Ui, engine: &Engine, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    let groups = engine.writing_evolution();
    if groups.is_empty() {
        widgets::hint(ui, &t!("discursive.history_empty"));
        return;
    }
    widgets::callout(ui, p.accent, |ui| {
        ui.label(t!("discursive.history_note"));
    });
    ui.add_space(8.0);
    for (g, group) in groups.iter().enumerate() {
        widgets::card(ui, |ui| {
            ui.label(
                RichText::new(t!(
                    "discursive.group_title",
                    source = i18n::rubric_source(group.rubric.source),
                    max = i18n::points(group.rubric.max_total())
                ))
                .strong(),
            );
            if groups.len() > 1 {
                widgets::hint(ui, &t!("discursive.group_apart"));
            }
            if group.mixed_graders {
                ui.colored_label(p.warning, t!("discursive.mixed_graders"));
            }
            egui::Grid::new(("evolution", g))
                .striped(true)
                .spacing(vec2(12.0, 4.0))
                .show(ui, |ui| {
                    ui.label(RichText::new(t!("discursive.col_attempt")).strong());
                    for c in &group.rubric.criteria {
                        ui.label(RichText::new(i18n::criterion_name(c)).small().strong());
                    }
                    ui.label(RichText::new(t!("discursive.col_total")).strong());
                    ui.end_row();
                    for row in &group.rows {
                        ui.add(
                            egui::Label::new(format!(
                                "{} · {}",
                                t!("discursive.attempt_title", number = row.number),
                                i18n::date(row.evaluated_at.date())
                            ))
                            .wrap_mode(egui::TextWrapMode::Extend),
                        )
                        .on_hover_text(&row.theme);
                        for share in &row.shares {
                            ui.label(format!("{:.0}%", share * 100.0));
                        }
                        ui.label(format!(
                            "{} / {}",
                            i18n::points(row.total),
                            i18n::points(row.max_total)
                        ));
                        ui.end_row();
                    }
                });
        });
        ui.add_space(8.0);
    }

    widgets::card(ui, |ui| {
        ui.label(RichText::new(t!("discursive.problems_by_source")).strong());
        widgets::hint(ui, &t!("discursive.problems_by_source_hint"));
        egui::Grid::new("writing_problems")
            .striped(true)
            .spacing(vec2(12.0, 4.0))
            .show(ui, |ui| {
                for key in [
                    t!("discursive.col_attempt"),
                    t!("discursive.col_reading"),
                    t!("discursive.category.writing"),
                    t!("discursive.category.content"),
                    t!("discursive.category.structure"),
                ] {
                    ui.label(RichText::new(key).strong());
                }
                ui.end_row();
                for (id, _) in engine.writing_evaluations() {
                    let (Some(problems), Some(attempt)) = (
                        engine.attempt_problems(*id),
                        engine.writing_attempts().iter().find(|a| a.id == *id),
                    ) else {
                        continue;
                    };
                    let theme = engine
                        .proposals()
                        .iter()
                        .find(|p| p.id == attempt.proposal_id)
                        .map_or("", |p| p.theme.as_str());
                    ui.label(format!(
                        "{} · {theme}",
                        t!("discursive.attempt_title", number = attempt.number)
                    ));
                    for n in [
                        problems.reading,
                        problems.writing,
                        problems.content,
                        problems.structure,
                    ] {
                        ui.label(n.to_string());
                    }
                    ui.end_row();
                }
            });
    });
    ui.add_space(8.0);

    widgets::card(ui, |ui| {
        ui.label(RichText::new(t!("discursive.recurring")).strong());
        ui.horizontal(|ui| {
            let busy = engine.is_analyzing_writing();
            if ui
                .add_enabled(
                    !busy && engine.writing_evaluations().len() >= 2,
                    egui::Button::new(t!("discursive.analyze")),
                )
                .clicked()
            {
                actions.send(Command::AnalyzeWriting);
            }
            if busy {
                ui.spinner();
            }
        });
        let Some(report) = engine.difficulty_report() else {
            widgets::hint(ui, &t!("discursive.recurring_hint"));
            return;
        };
        widgets::hint(
            ui,
            &t!(
                "discursive.recurring_meta",
                count = report.attempts.len(),
                date = i18n::date(report.created_at.date())
            ),
        );
        if report.issues.is_empty() {
            widgets::hint(ui, &t!("discursive.recurring_none"));
        }
        for issue in &report.issues {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                widgets::chip(ui, &i18n::problem_category(issue.category), p.warning);
                ui.label(RichText::new(&issue.label).strong());
            });
            ui.label(&issue.advice);
            for example in &issue.examples {
                let number = engine
                    .writing_attempts()
                    .iter()
                    .find(|a| a.id == example.attempt)
                    .map_or(0, |a| a.number);
                ui.label(
                    RichText::new(t!(
                        "discursive.example",
                        number = number,
                        quote = example.quote
                    ))
                    .italics()
                    .small()
                    .color(p.muted),
                );
            }
        }
    });
}
