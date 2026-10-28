//! Exam board material: upload past exams and board handouts, see what was
//! used and what was set aside, and move items by hand.

use std::collections::HashMap;

use egui::RichText;
use kg_engine::{
    BoardDocument, BoardItem, Command, DocumentId, DocumentKind, Engine, Relevance, StudyPlan,
    TopicId,
};
use rust_i18n::t;

use super::Actions;
use crate::app::{KnowGiverApp, Screen};
use crate::{i18n, theme, widgets};

/// Longest item text shown in the lists.
const PREVIEW_CHARS: usize = 220;

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    let state = &mut app.ui;
    widgets::header(ui, &t!("board.title"), &t!("board.subtitle"));

    let Some(plan) = engine.active_plan() else {
        widgets::hint(ui, &t!("plan.empty"));
        if ui.button(t!("plan.go_to_edital")).clicked() {
            actions.go_to(Screen::Edital);
        }
        actions.run(app);
        return;
    };

    egui::ScrollArea::vertical().show(ui, |ui| {
        upload_card(ui, engine, &mut actions);
        ui.add_space(12.0);

        if engine.board_documents().is_empty() {
            widgets::hint(ui, &t!("board.no_documents"));
        }
        for document in engine.board_documents() {
            document_card(
                ui,
                document,
                engine.board_job().is_none(),
                &mut state.confirm_delete_doc,
                &mut actions,
            );
        }

        ui.add_space(12.0);
        item_lists(ui, engine, plan, &mut state.board_choice, &mut actions);
    });
    actions.run(app);
}

fn upload_card(ui: &mut egui::Ui, engine: &Engine, actions: &mut Actions) {
    widgets::card(ui, |ui| {
        widgets::hint(ui, &t!("board.upload_hint"));
        ui.add_space(4.0);
        match engine.board_job() {
            Some(job) => {
                let (done, total) = job.progress();
                ui.horizontal(|ui| {
                    ui.spinner();
                    let text = if total == 0 {
                        t!("board.reading", file = job.file)
                    } else {
                        t!(
                            "board.analyzing",
                            file = job.file,
                            done = done,
                            total = total
                        )
                    };
                    ui.label(text);
                    if ui.small_button(t!("common.cancel")).clicked() {
                        actions.send(Command::CancelBoard);
                    }
                });
                if total > 0 {
                    ui.add(egui::ProgressBar::new(done as f32 / total as f32).desired_height(6.0));
                }
            }
            None => {
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, &t!("board.add_exam")).clicked()
                        && let Some(path) = pick_pdf(&t!("board.add_exam"))
                    {
                        actions.send(Command::ImportBoard {
                            path,
                            kind: DocumentKind::Exam,
                        });
                    }
                    if ui.button(t!("board.add_material")).clicked()
                        && let Some(path) = pick_pdf(&t!("board.add_material"))
                    {
                        actions.send(Command::ImportBoard {
                            path,
                            kind: DocumentKind::Material,
                        });
                    }
                });
            }
        }
    });
}

fn document_card(
    ui: &mut egui::Ui,
    document: &BoardDocument,
    idle: bool,
    confirm_delete: &mut Option<DocumentId>,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&document.file_name).strong());
                    widgets::chip(ui, &kind_label(document.kind), p.accent);
                });
                ui.label(
                    RichText::new(t!(
                        "board.doc_stats",
                        date = i18n::date(document.added_on),
                        used = document.used,
                        set_aside = document.set_aside
                    ))
                    .small()
                    .color(p.muted),
                );
                if document.kind == DocumentKind::Exam && document.unanswered > 0 {
                    ui.label(
                        RichText::new(t!("board.unanswered", count = document.unanswered))
                            .small()
                            .color(p.warning),
                    );
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t!("edital.delete")).clicked() {
                    *confirm_delete = Some(document.id);
                }
                if document.kind == DocumentKind::Exam
                    && ui
                        .add_enabled(idle, egui::Button::new(t!("board.add_key")))
                        .on_hover_text(t!("board.add_key_hint"))
                        .clicked()
                    && let Some(path) = pick_pdf(&t!("board.add_key"))
                {
                    actions.send(Command::ImportAnswerKey {
                        path,
                        exam: document.id,
                    });
                }
            });
        });
        if *confirm_delete == Some(document.id) {
            widgets::callout(ui, p.danger, |ui| {
                ui.label(t!("board.confirm_delete"));
                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new(t!("edital.delete")).color(p.danger))
                        .clicked()
                    {
                        actions.send(Command::DeleteBoardDocument(document.id));
                        *confirm_delete = None;
                    }
                    if ui.button(t!("common.cancel")).clicked() {
                        *confirm_delete = None;
                    }
                });
            });
        }
    });
}

/// One row in the used / set-aside lists.
struct Row<'a> {
    item: BoardItem,
    text: &'a str,
    reason: &'a str,
    topic: Option<TopicId>,
    relevance: Relevance,
    is_question: bool,
}

fn item_lists(
    ui: &mut egui::Ui,
    engine: &Engine,
    plan: &StudyPlan,
    choice: &mut HashMap<BoardItem, TopicId>,
    actions: &mut Actions,
) {
    let rows: Vec<Row<'_>> = engine
        .board_questions()
        .iter()
        .map(|q| Row {
            item: BoardItem::Question(q.id),
            text: &q.prompt,
            reason: &q.reason,
            topic: q.topic_id,
            relevance: q.relevance,
            is_question: true,
        })
        .chain(engine.board_excerpts().iter().map(|e| Row {
            item: BoardItem::Excerpt(e.id),
            text: &e.summary,
            reason: &e.reason,
            topic: e.topic_id,
            relevance: e.relevance,
            is_question: false,
        }))
        .collect();
    if rows.is_empty() {
        return;
    }
    let (used, set_aside): (Vec<&Row<'_>>, Vec<&Row<'_>>) =
        rows.iter().partition(|r| r.relevance.is_used());

    egui::CollapsingHeader::new(
        RichText::new(t!("board.set_aside", count = set_aside.len())).strong(),
    )
    .id_salt("board_set_aside")
    .default_open(!set_aside.is_empty())
    .show(ui, |ui| {
        widgets::hint(ui, &t!("board.set_aside_hint"));
        for row in &set_aside {
            item_row(ui, plan, row, choice, actions);
        }
    });
    egui::CollapsingHeader::new(RichText::new(t!("board.used", count = used.len())).strong())
        .id_salt("board_used")
        .show(ui, |ui| {
            for row in &used {
                item_row(ui, plan, row, choice, actions);
            }
        });
}

fn item_row(
    ui: &mut egui::Ui,
    plan: &StudyPlan,
    row: &Row<'_>,
    choice: &mut HashMap<BoardItem, TopicId>,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    let preview: String = row.text.chars().take(PREVIEW_CHARS).collect();
    let ellipsis = if row.text.chars().count() > PREVIEW_CHARS {
        "…"
    } else {
        ""
    };
    ui.horizontal(|ui| {
        let kind = if row.is_question {
            t!("board.item_question")
        } else {
            t!("board.item_excerpt")
        };
        widgets::chip(ui, &kind, p.muted);
        ui.label(format!("{preview}{ellipsis}"));
    });
    ui.horizontal(|ui| {
        if row.relevance.is_used() {
            let topic = row
                .topic
                .and_then(|id| plan.find_topic(id))
                .map(|(_, t)| t.title.clone())
                .unwrap_or_default();
            ui.label(RichText::new(topic).small().color(p.accent));
            if row.relevance == Relevance::Partial {
                widgets::chip(ui, &t!("board.partial"), p.warning);
            }
            if ui.small_button(t!("board.set_aside_action")).clicked() {
                actions.send(Command::SetBoardItem {
                    item: row.item,
                    relevance: Relevance::OutOfScope,
                    topic: None,
                });
            }
        } else {
            if !row.reason.is_empty() {
                ui.label(RichText::new(row.reason).small().color(p.muted));
            }
            let selected = choice.get(&row.item).copied().or(row.topic);
            let selected_title = selected
                .and_then(|id| plan.find_topic(id))
                .map(|(_, t)| t.title.clone())
                .unwrap_or_else(|| t!("board.pick_topic").into_owned());
            egui::ComboBox::from_id_salt(("board_topic", format!("{:?}", row.item)))
                .width(240.0)
                .selected_text(RichText::new(selected_title).small())
                .show_ui(ui, |ui| {
                    for (subject, topic) in plan.topics() {
                        let label = format!("{} — {}", subject.name, topic.title);
                        if ui
                            .selectable_label(selected == Some(topic.id), label)
                            .clicked()
                        {
                            choice.insert(row.item, topic.id);
                        }
                    }
                });
            if ui
                .add_enabled(
                    selected.is_some(),
                    egui::Button::new(t!("board.use_anyway")).small(),
                )
                .clicked()
            {
                actions.send(Command::SetBoardItem {
                    item: row.item,
                    relevance: Relevance::Relevant,
                    topic: selected,
                });
                choice.remove(&row.item);
            }
        }
    });
    ui.separator();
}

fn kind_label(kind: DocumentKind) -> String {
    match kind {
        DocumentKind::Exam => t!("board.kind.exam"),
        DocumentKind::AnswerKey => t!("board.kind.answer_key"),
        DocumentKind::Material => t!("board.kind.material"),
    }
    .into_owned()
}

fn pick_pdf(title: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter(t!("edital.pdf_filter"), &["pdf"])
        .pick_file()
}
