//! Edital import, plan generation and the list of saved plans.

use egui::RichText;
use kg_engine::{Command, Edital, ImportState, PlanPrefs, PlanSummary};
use rust_i18n::t;

use super::Actions;
use crate::app::{KnowGiverApp, Screen};
use crate::{i18n, theme, widgets};

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    let state = &mut app.ui;
    widgets::header(ui, &t!("edital.title"), &t!("edital.subtitle"));

    egui::ScrollArea::vertical().show(ui, |ui| {
        match engine.import_state() {
            ImportState::Extracting { file } => {
                widgets::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t!("edital.extracting", file = file));
                    });
                });
            }
            ImportState::Ready(edital) => {
                ready_card(
                    ui,
                    edital,
                    &mut state.prefs,
                    engine.generating_since(),
                    &mut actions,
                );
            }
            ImportState::Empty | ImportState::Failed => {
                widgets::card(ui, |ui| {
                    widgets::hint(ui, &t!("edital.empty"));
                    ui.add_space(4.0);
                    if widgets::primary_button(ui, &t!("edital.select_pdf")).clicked()
                        && let Some(path) = pick_pdf()
                    {
                        actions.send(Command::ImportEdital(path));
                    }
                });
            }
        }

        ui.add_space(20.0);
        ui.label(RichText::new(t!("edital.your_plans")).strong().size(18.0));
        if engine.plans().is_empty() {
            widgets::hint(ui, &t!("edital.no_plans"));
        }
        let active = engine.active_plan().map(|p| p.id);
        for summary in engine.plans() {
            plan_row(
                ui,
                summary,
                active == Some(summary.id),
                &mut state.confirm_delete,
                &mut actions,
            );
        }
    });
    actions.run(app);
}

fn ready_card(
    ui: &mut egui::Ui,
    edital: &Edital,
    prefs: &mut PlanPrefs,
    generating_since: Option<std::time::Instant>,
    actions: &mut Actions,
) {
    widgets::card(ui, |ui| {
        ui.label(RichText::new(&edital.file_name).strong());
        widgets::hint(
            ui,
            &t!(
                "edital.summary",
                pages = edital.pages,
                chars = edital.text.char_count()
            ),
        );
        egui::CollapsingHeader::new(t!("edital.preview"))
            .id_salt("edital_preview")
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        let preview: String = edital.text.as_str().chars().take(3_000).collect();
                        ui.label(RichText::new(preview).small());
                    });
            });

        ui.add_space(8.0);
        egui::Grid::new("plan_prefs")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .show(ui, |ui| {
                ui.label(t!("edital.weeks"));
                ui.add(
                    egui::DragValue::new(&mut prefs.weeks)
                        .range(1..=104)
                        .speed(0.2),
                );
                ui.end_row();

                ui.label(t!("edital.hours"));
                ui.add(
                    egui::DragValue::new(&mut prefs.hours_per_week)
                        .range(1..=80)
                        .speed(0.2),
                );
                ui.end_row();

                ui.label(t!("edital.role"));
                ui.add(
                    egui::TextEdit::singleline(&mut prefs.role)
                        .hint_text(t!("edital.role_hint"))
                        .desired_width(360.0),
                );
                ui.end_row();
            });

        ui.add_space(8.0);
        match generating_since {
            Some(started) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t!(
                        "edital.generating",
                        seconds = started.elapsed().as_secs()
                    ));
                });
            }
            None => {
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, &t!("edital.generate")).clicked() {
                        actions.send(Command::GeneratePlan(prefs.clone()));
                    }
                    if ui.button(t!("edital.other_file")).clicked()
                        && let Some(path) = pick_pdf()
                    {
                        actions.send(Command::ImportEdital(path));
                    }
                });
            }
        }
    });
}

fn plan_row(
    ui: &mut egui::Ui,
    summary: &PlanSummary,
    is_active: bool,
    confirm_delete: &mut Option<kg_engine::PlanId>,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&summary.title).strong());
                    if is_active {
                        widgets::chip(ui, &t!("edital.active"), p.accent);
                    }
                });
                if !summary.exam.is_empty() {
                    widgets::hint(ui, &summary.exam);
                }
                ui.label(
                    RichText::new(t!(
                        "edital.plan_meta",
                        date = i18n::date(summary.created_on),
                        done = summary.done_count,
                        total = summary.topic_count
                    ))
                    .small()
                    .color(p.muted),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t!("edital.delete")).clicked() {
                    *confirm_delete = Some(summary.id);
                }
                if ui.button(t!("edital.open")).clicked() {
                    actions.send(Command::SelectPlan(summary.id));
                    actions.go_to(Screen::Plan);
                }
            });
        });
        if *confirm_delete == Some(summary.id) {
            widgets::callout(ui, p.danger, |ui| {
                ui.label(t!("edital.confirm_delete"));
                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new(t!("edital.delete")).color(p.danger))
                        .clicked()
                    {
                        actions.send(Command::DeletePlan(summary.id));
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

fn pick_pdf() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .set_title(t!("edital.select_pdf"))
        .add_filter(t!("edital.pdf_filter"), &["pdf"])
        .pick_file()
}
