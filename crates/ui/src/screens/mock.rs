//! Mock exams: the exam's format, writing and printing a mock exam, the
//! timed sitting and its result. The clock, the key and the score live in
//! the engine; this screen only shows them and sends the answers.

use std::time::Duration;

use egui::{RichText, Stroke};
use kg_engine::{
    AnswerStyle, Command, Engine, ExamFormat, ExamLabels, FormatSource, MockAttempt, MockExam,
    MockExamId, MockPrint, MockResult,
};
use rust_i18n::t;

use super::Actions;
use crate::app::{KnowGiverApp, Screen};
use crate::icons::Icon;
use crate::{i18n, theme, widgets};

/// Time left at which the clock turns amber.
const WARN_SECONDS: i64 = 15 * 60;

/// What this screen keeps between frames.
#[derive(Debug, Clone, Default)]
struct MockUi {
    /// The format being edited, until saved or dropped.
    draft: Option<ExamFormat>,
    /// Answer card only, for an exam taken on paper.
    paper: bool,
    confirm_finish: bool,
    confirm_delete: Option<MockExamId>,
}

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    let id = egui::Id::new("mock_screen");
    let mut state: MockUi = ui
        .ctx()
        .data_mut(|d| d.get_temp::<MockUi>(id))
        .unwrap_or_default();

    if engine.active_plan().is_none() {
        if widgets::empty_state(
            ui,
            Icon::Timer,
            &t!("mock.title"),
            &t!("mock.no_plan"),
            Some(&t!("mock.go_to_edital")),
        ) {
            actions.go_to(Screen::Edital);
        }
    } else if let Some((exam, attempt)) = engine.open_mock() {
        if attempt.finished_at.is_none() {
            sitting(ui, exam, attempt, &mut state, &mut actions);
        } else if let Some(result) = engine.mock_result(attempt) {
            result_view(ui, exam, attempt, &result, &mut actions);
        }
    } else {
        state.confirm_finish = false;
        home(ui, engine, &mut state, &mut actions);
    }

    ui.ctx().data_mut(|d| d.insert_temp(id, state));
    actions.run(app);
}

// ---------------------------------------------------------------- home

fn home(ui: &mut egui::Ui, engine: &Engine, state: &mut MockUi, actions: &mut Actions) {
    widgets::header(ui, &t!("mock.title"), &t!("mock.subtitle"));
    egui::ScrollArea::vertical().show(ui, |ui| {
        if let Some(running) = engine.running_mock()
            && let Some(exam) = engine.mock_exams().iter().find(|e| e.id == running.exam_id)
        {
            let p = theme::palette(ui.ctx());
            widgets::callout(ui, p.warning, |ui| {
                ui.horizontal(|ui| {
                    let left = running.remaining(&exam.format, now());
                    ui.label(
                        RichText::new(t!(
                            "mock.running",
                            title = exam.title,
                            time = clock(left.num_seconds())
                        ))
                        .strong(),
                    );
                    if widgets::primary_button(ui, &t!("mock.back_to_exam")).clicked() {
                        actions.send(Command::OpenMockAttempt(running.id));
                    }
                });
            });
            ui.add_space(12.0);
        }
        if let Some(format) = engine.exam_format() {
            format_card(ui, engine, &format, state, actions);
            ui.add_space(12.0);
            generate_card(ui, engine, &format, actions);
        }
        ui.add_space(16.0);
        if engine.mock_exams().is_empty() {
            widgets::hint(ui, &t!("mock.none_yet"));
        }
        for exam in engine.mock_exams() {
            exam_card(ui, engine, exam, state, actions);
            ui.add_space(10.0);
        }
    });
}

fn format_card(
    ui: &mut egui::Ui,
    engine: &Engine,
    format: &ExamFormat,
    state: &mut MockUi,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            widgets::kicker(ui, &t!("mock.format"), p.muted);
            let (label, color) = match format.source {
                FormatSource::Notice => (t!("mock.source_notice"), p.success),
                FormatSource::Estimate => (t!("mock.source_estimate"), p.warning),
                FormatSource::Student => (t!("mock.source_student"), p.accent),
            };
            widgets::chip(ui, &label, color);
        });
        ui.label(RichText::new(format_summary(format)).strong());
        if format.source == FormatSource::Estimate {
            widgets::hint(ui, &t!("mock.estimate_hint"));
        }
        ui.add_space(6.0);
        if let Some(draft) = state.draft.as_mut() {
            let writing = engine.writing_rules().has_written_test();
            let mut save = false;
            let mut drop = false;
            format_editor(ui, draft, writing, &mut save, &mut drop);
            if save {
                actions.send(Command::SaveExamFormat(draft.clone()));
                state.draft = None;
            } else if drop {
                state.draft = None;
            }
        } else {
            ui.horizontal(|ui| {
                if engine.reading_exam_format() {
                    ui.spinner();
                    ui.label(t!("mock.reading_format"));
                } else if ui
                    .button(t!("mock.read_format"))
                    .on_hover_text(t!("mock.read_format_hint"))
                    .clicked()
                {
                    actions.send(Command::ReadExamFormat);
                }
                if ui.button(t!("mock.adjust")).clicked() {
                    state.draft = Some(format.clone());
                }
            });
        }
    });
}

fn format_editor(
    ui: &mut egui::Ui,
    draft: &mut ExamFormat,
    writing: bool,
    save: &mut bool,
    drop: &mut bool,
) {
    egui::Grid::new("mock_format_grid")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(t!("mock.minutes_label"));
            ui.add(
                egui::DragValue::new(&mut draft.minutes)
                    .range(10..=600)
                    .speed(5)
                    .suffix(" min"),
            );
            ui.end_row();

            ui.label(t!("mock.style_label"));
            ui.horizontal(|ui| {
                let choice = matches!(draft.style, AnswerStyle::Choice { .. });
                if ui
                    .selectable_label(choice, t!("mock.style_choice_short"))
                    .clicked()
                    && !choice
                {
                    draft.style = AnswerStyle::Choice { options: 5 };
                }
                if ui
                    .selectable_label(!choice, t!("mock.style_true_false"))
                    .clicked()
                {
                    draft.style = AnswerStyle::TrueFalse;
                }
                if let AnswerStyle::Choice { options } = &mut draft.style {
                    ui.add(
                        egui::DragValue::new(options)
                            .range(2..=5)
                            .suffix(format!(" {}", t!("mock.options_suffix"))),
                    );
                }
            });
            ui.end_row();

            ui.label(t!("mock.wrong_cancels_label"));
            ui.checkbox(&mut draft.wrong_cancels, t!("mock.wrong_cancels"));
            ui.end_row();

            ui.label(t!("mock.essay_label"));
            ui.add_enabled_ui(writing, |ui| {
                ui.checkbox(&mut draft.essay, t!("mock.essay"))
                    .on_disabled_hover_text(t!("mock.essay_none"));
            });
            if !writing {
                draft.essay = false;
            }
            ui.end_row();
        });
    ui.add_space(8.0);
    egui::Grid::new("mock_sections_grid")
        .num_columns(3)
        .striped(true)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.label(RichText::new(t!("mock.subject")).small());
            ui.label(RichText::new(t!("mock.questions")).small());
            ui.label(RichText::new(t!("mock.weight")).small());
            ui.end_row();
            for section in &mut draft.sections {
                ui.label(&section.subject);
                ui.add(egui::DragValue::new(&mut section.questions).range(0..=100));
                ui.add(
                    egui::DragValue::new(&mut section.weight)
                        .range(0.1..=10.0)
                        .speed(0.1)
                        .max_decimals(2),
                );
                ui.end_row();
            }
        });
    let total = draft.total_questions();
    let valid = draft.validate().is_ok();
    ui.add_space(6.0);
    ui.label(t!(
        "mock.total",
        total = total,
        points = i18n::points(draft.max_points())
    ));
    if !valid {
        let p = theme::palette(ui.ctx());
        ui.label(RichText::new(t!("error.mock.format")).color(p.danger));
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(valid, egui::Button::new(t!("mock.save_format")))
            .clicked()
        {
            *save = true;
        }
        if ui.button(t!("mock.cancel")).clicked() {
            *drop = true;
        }
    });
}

fn generate_card(ui: &mut egui::Ui, engine: &Engine, format: &ExamFormat, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    if let Some(job) = engine.mock_job() {
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(t!("mock.writing")).strong());
            });
            widgets::progress_line(
                ui,
                240.0,
                job.done() as f32 / job.total.max(1) as f32,
                &format!("{}/{}", job.done(), job.total),
                p.accent,
            );
            widgets::hint(ui, &t!("mock.writing_hint"));
            if ui.button(t!("mock.cancel")).clicked() {
                actions.send(Command::CancelMock);
            }
        });
        return;
    }
    ui.horizontal(|ui| {
        let ready = format.validate().is_ok();
        let button = ui.add_enabled_ui(ready, |ui| {
            widgets::primary_button(ui, &t!("mock.generate"))
        });
        if button.inner.clicked() {
            let title = t!("mock.default_title", n = engine.mock_exams().len() + 1);
            actions.send(Command::GenerateMock(title.into_owned()));
        }
        widgets::hint(ui, &t!("mock.generate_hint"));
    });
}

fn exam_card(
    ui: &mut egui::Ui,
    engine: &Engine,
    exam: &MockExam,
    state: &mut MockUi,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            widgets::kicker(ui, &i18n::date(exam.created_at.date()), p.muted);
            widgets::kicker(ui, &kg_engine::mock_code(exam.id), p.muted);
        });
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&exam.title)
                    .family(theme::heading_family())
                    .size(19.0),
            );
            if exam.proposal.is_some() {
                widgets::chip(ui, &t!("mock.with_essay"), p.accent);
            }
        });
        ui.label(RichText::new(format_summary(&exam.format)).color(p.muted));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if widgets::primary_button(ui, &t!("mock.start"))
                .on_hover_text(t!("mock.start_hint"))
                .clicked()
            {
                actions.send(Command::StartMock(exam.id));
            }
            let printing = engine.printing_mock();
            if ui
                .add_enabled(!printing, egui::Button::new(t!("mock.print")))
                .on_hover_text(t!("mock.print_hint"))
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_title(t!("mock.print"))
                    .set_file_name(format!(
                        "{}-{}.pdf",
                        kg_engine::slug(&exam.title),
                        kg_engine::mock_code(exam.id)
                    ))
                    .add_filter(t!("handout.pdf_filter"), &["pdf"])
                    .save_file()
            {
                let plan = engine
                    .active_plan()
                    .map(|p| p.exam.clone())
                    .unwrap_or_default();
                actions.send(Command::PrintMock(Box::new(print_request(
                    exam, &plan, path,
                ))));
            }
            if printing {
                ui.spinner();
            }
            if state.confirm_delete == Some(exam.id) {
                ui.label(t!("mock.confirm_delete"));
                if ui.button(t!("mock.delete")).clicked() {
                    actions.send(Command::DeleteMock(exam.id));
                    state.confirm_delete = None;
                }
                if ui.button(t!("mock.cancel")).clicked() {
                    state.confirm_delete = None;
                }
            } else if ui.button(t!("mock.delete")).clicked() {
                state.confirm_delete = Some(exam.id);
            }
        });
        let attempts: Vec<&MockAttempt> = engine
            .mock_attempts()
            .iter()
            .filter(|a| a.exam_id == exam.id && a.finished_at.is_some())
            .collect();
        if !attempts.is_empty() {
            ui.add_space(6.0);
            for attempt in attempts {
                let Some(result) = engine.mock_result(attempt) else {
                    continue;
                };
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(i18n::date(attempt.started_at.date()))
                            .small()
                            .color(p.muted),
                    );
                    widgets::progress_line(
                        ui,
                        120.0,
                        result.share() as f32,
                        &score_text(&result),
                        score_color(ui, result.share()),
                    );
                    if let Some(used) = attempt.time_used() {
                        ui.label(
                            RichText::new(duration_text(used.num_minutes().max(0) as u32))
                                .small()
                                .color(p.muted),
                        );
                    }
                    if ui.small_button(t!("mock.see_result")).clicked() {
                        actions.send(Command::OpenMockAttempt(attempt.id));
                    }
                    if ui
                        .small_button("×")
                        .on_hover_text(t!("mock.delete_attempt"))
                        .clicked()
                    {
                        actions.send(Command::DeleteMockAttempt(attempt.id));
                    }
                });
            }
        }
    });
}

// ------------------------------------------------------------- sitting

fn sitting(
    ui: &mut egui::Ui,
    exam: &MockExam,
    attempt: &MockAttempt,
    state: &mut MockUi,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    ui.ctx().request_repaint_after(Duration::from_secs(1));
    let left = attempt.remaining(&exam.format, now()).num_seconds();
    let marks = marks(exam.format.style);
    let answered = attempt.answers.iter().filter(|a| a.is_some()).count();
    let total = exam.questions.len();

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            widgets::kicker(ui, &t!("mock.sitting"), p.muted);
            ui.label(
                RichText::new(&exam.title)
                    .family(theme::heading_family())
                    .size(22.0),
            );
            ui.label(
                RichText::new(t!("mock.answered", n = answered, total = total)).color(p.muted),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (text, color) = if left >= 0 {
                let color = if left <= WARN_SECONDS {
                    p.warning
                } else {
                    p.accent
                };
                (clock(left), color)
            } else {
                (
                    t!("mock.time_up", over = clock(-left)).into_owned(),
                    p.danger,
                )
            };
            ui.label(
                RichText::new(text)
                    .family(theme::mono_family())
                    .size(30.0)
                    .strong()
                    .color(color),
            );
        });
    });
    widgets::rule(ui, 3.0);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui
            .selectable_label(!state.paper, t!("mock.on_screen"))
            .clicked()
        {
            state.paper = false;
        }
        if ui
            .selectable_label(state.paper, t!("mock.card_only"))
            .on_hover_text(t!("mock.card_only_hint"))
            .clicked()
        {
            state.paper = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::primary_button(ui, &t!("mock.finish")).clicked() {
                state.confirm_finish = true;
            }
            if ui
                .button(t!("mock.leave"))
                .on_hover_text(t!("mock.leave_hint"))
                .clicked()
            {
                actions.send(Command::CloseMock);
            }
        });
    });
    if left < 0 {
        widgets::callout(ui, p.danger, |ui| {
            ui.label(t!("mock.time_up_hint"));
        });
    }
    if state.confirm_finish {
        widgets::callout(ui, p.accent, |ui| {
            ui.label(RichText::new(t!("mock.confirm_finish", blank = total - answered)).strong());
            ui.horizontal(|ui| {
                if widgets::primary_button(ui, &t!("mock.finish_now")).clicked() {
                    actions.send(Command::FinishMock(true_false_words()));
                    state.confirm_finish = false;
                }
                if ui.button(t!("mock.keep_going")).clicked() {
                    state.confirm_finish = false;
                }
            });
        });
    }
    ui.add_space(8.0);
    egui::ScrollArea::vertical()
        .id_salt("mock_sitting")
        .show(ui, |ui| {
            if state.paper {
                answer_card(ui, exam, attempt, &marks, actions);
            } else {
                booklet(ui, exam, attempt, &marks, actions);
            }
        });
}

fn booklet(
    ui: &mut egui::Ui,
    exam: &MockExam,
    attempt: &MockAttempt,
    marks: &[String],
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    let mut section = usize::MAX;
    for (n, question) in exam.questions.iter().enumerate() {
        if question.section != section {
            section = question.section;
            ui.add_space(10.0);
            if let Some(s) = exam.format.sections.get(section) {
                widgets::kicker(ui, &s.subject, p.accent);
            }
        }
        let chosen = attempt.answers.get(n).copied().flatten();
        widgets::card(ui, |ui| {
            ui.label(RichText::new(format!("{}. {}", n + 1, question.prompt)).size(15.0));
            ui.add_space(4.0);
            let labels: Vec<String> = if question.options.is_empty() {
                true_false_words().to_vec()
            } else {
                question
                    .options
                    .iter()
                    .enumerate()
                    .map(|(i, o)| format!("({}) {o}", marks.get(i).map_or("?", String::as_str)))
                    .collect()
            };
            for (i, label) in labels.iter().enumerate() {
                let selected = chosen == Some(i as u8);
                if ui
                    .add(
                        egui::Button::selectable(selected, RichText::new(label))
                            .wrap()
                            .min_size(egui::vec2(ui.available_width(), 0.0)),
                    )
                    .clicked()
                {
                    actions.send(Command::MarkMockAnswer {
                        question: n,
                        answer: (!selected).then_some(i as u8),
                    });
                }
            }
        });
        ui.add_space(6.0);
    }
}

fn answer_card(
    ui: &mut egui::Ui,
    exam: &MockExam,
    attempt: &MockAttempt,
    marks: &[String],
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    widgets::hint(ui, &t!("mock.card_hint"));
    ui.add_space(6.0);
    let count = usize::from(exam.format.style.marks());
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(18.0, 6.0);
        for n in 0..exam.questions.len() {
            let chosen = attempt.answers.get(n).copied().flatten();
            ui.allocate_ui(egui::vec2(40.0 + 30.0 * count as f32, 26.0), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.add_sized(
                        [28.0, 22.0],
                        egui::Label::new(RichText::new((n + 1).to_string()).strong()),
                    );
                    for (i, mark) in marks.iter().take(count).enumerate() {
                        let selected = chosen == Some(i as u8);
                        let button = egui::Button::new(RichText::new(mark).small())
                            .min_size(egui::vec2(24.0, 22.0))
                            .corner_radius(11)
                            .fill(if selected { p.accent } else { p.surface })
                            .stroke(Stroke::new(1.0, p.ink.gamma_multiply(0.6)));
                        if ui.add(button).clicked() {
                            actions.send(Command::MarkMockAnswer {
                                question: n,
                                answer: (!selected).then_some(i as u8),
                            });
                        }
                    }
                });
            });
        }
    });
}

// -------------------------------------------------------------- result

fn result_view(
    ui: &mut egui::Ui,
    exam: &MockExam,
    attempt: &MockAttempt,
    result: &MockResult,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    let marks = marks(exam.format.style);
    ui.horizontal(|ui| {
        if ui.button(t!("mock.back")).clicked() {
            actions.send(Command::CloseMock);
        }
    });
    widgets::header(ui, &exam.title, &t!("mock.result_subtitle"));
    egui::ScrollArea::vertical().show(ui, |ui| {
        widgets::card(ui, |ui| {
            widgets::kicker(ui, &t!("mock.score"), p.muted);
            ui.label(
                RichText::new(t!(
                    "mock.points",
                    points = i18n::points(result.points),
                    max = i18n::points(result.max)
                ))
                .family(theme::heading_family())
                .size(30.0)
                .color(score_color(ui, result.share())),
            );
            ui.horizontal(|ui| {
                widgets::chip(ui, &t!("mock.right", n = result.right), p.success);
                widgets::chip(ui, &t!("mock.wrong", n = result.wrong), p.danger);
                widgets::chip(ui, &t!("mock.blank", n = result.blank), p.muted);
                if let Some(used) = attempt.time_used() {
                    widgets::chip(
                        ui,
                        &t!(
                            "mock.time_used",
                            time = duration_text(used.num_minutes().max(0) as u32)
                        ),
                        p.accent,
                    );
                }
                if attempt.overtime(&exam.format) {
                    widgets::chip(ui, &t!("mock.overtime"), p.warning);
                }
            });
            if exam.format.wrong_cancels {
                widgets::hint(ui, &t!("mock.wrong_cancels_note"));
            }
            ui.add_space(8.0);
            egui::Grid::new("mock_result_sections")
                .num_columns(3)
                .spacing([16.0, 6.0])
                .show(ui, |ui| {
                    for section in &result.sections {
                        let name = exam
                            .format
                            .sections
                            .get(section.section)
                            .map_or("", |s| s.subject.as_str());
                        ui.label(name);
                        let share = if section.max > 0.0 {
                            (section.points / section.max).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        widgets::progress_line(
                            ui,
                            160.0,
                            share as f32,
                            &format!(
                                "{}/{}",
                                i18n::points(section.points),
                                i18n::points(section.max)
                            ),
                            score_color(ui, share),
                        );
                        ui.label(
                            RichText::new(t!(
                                "mock.section_counts",
                                right = section.right,
                                wrong = section.wrong,
                                blank = section.blank
                            ))
                            .small()
                            .color(p.muted),
                        );
                        ui.end_row();
                    }
                });
        });
        ui.add_space(10.0);
        if let Some(proposal) = exam.proposal {
            widgets::callout(ui, p.accent, |ui| {
                ui.label(RichText::new(t!("mock.essay_next")).strong());
                widgets::hint(ui, &t!("mock.essay_next_hint"));
                if ui.button(t!("mock.essay_send")).clicked() {
                    actions.send(Command::StartAttempt(proposal));
                    actions.go_to(Screen::Discursive);
                }
            });
            ui.add_space(10.0);
        }
        if result.wrong > 0 {
            ui.horizontal(|ui| {
                widgets::hint(ui, &t!("mock.mistakes_saved"));
                if ui.button(t!("mock.open_notebook")).clicked() {
                    actions.go_to(Screen::Quiz);
                }
            });
            ui.add_space(10.0);
        }
        egui::CollapsingHeader::new(t!("mock.key"))
            .default_open(false)
            .show(ui, |ui| {
                for (n, question) in exam.questions.iter().enumerate() {
                    let chosen = attempt.answers.get(n).copied().flatten();
                    let right = chosen == Some(question.correct);
                    let answer_text = |answer: u8| answer_label(question, answer, &marks);
                    ui.add_space(6.0);
                    ui.label(RichText::new(format!("{}. {}", n + 1, question.prompt)).strong());
                    let (yours, color) = match chosen {
                        Some(a) if right => (answer_text(a), p.success),
                        Some(a) => (answer_text(a), p.danger),
                        None => (t!("mock.left_blank").into_owned(), p.muted),
                    };
                    ui.label(RichText::new(t!("mock.your_answer", answer = yours)).color(color));
                    if !right {
                        ui.label(
                            RichText::new(t!(
                                "mock.right_answer",
                                answer = answer_text(question.correct)
                            ))
                            .color(p.success),
                        );
                    }
                    if !question.explanation.is_empty() {
                        widgets::hint(ui, &question.explanation);
                    }
                }
            });
    });
}

// ------------------------------------------------------------- helpers

fn now() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

/// "02:13:05" from seconds.
fn clock(seconds: i64) -> String {
    let s = seconds.max(0);
    format!("{:02}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
}

/// "4 h 30 min", "45 min", "2 h".
pub fn duration_text(minutes: u32) -> String {
    let (h, m) = (minutes / 60, minutes % 60);
    match (h, m) {
        (0, m) => t!("mock.duration_minutes", m = m),
        (h, 0) => t!("mock.duration_hours", h = h),
        (h, m) => t!("mock.duration_both", h = h, m = m),
    }
    .into_owned()
}

fn style_text(style: AnswerStyle) -> String {
    match style {
        AnswerStyle::Choice { options } => t!("mock.style_choice", n = options),
        AnswerStyle::TrueFalse => t!("mock.style_true_false"),
    }
    .into_owned()
}

fn format_summary(format: &ExamFormat) -> String {
    let mut parts = vec![
        t!("mock.n_questions", n = format.total_questions()).into_owned(),
        duration_text(format.minutes),
        style_text(format.style),
    ];
    if format.wrong_cancels {
        parts.push(t!("mock.wrong_cancels_short").into_owned());
    }
    if format.essay {
        parts.push(t!("mock.with_essay").into_owned());
    }
    parts.join(" · ")
}

/// Letters for a question's options or for true/false.
fn marks(style: AnswerStyle) -> Vec<String> {
    match style {
        AnswerStyle::Choice { options } => choice_marks()
            .into_iter()
            .take(usize::from(options))
            .collect(),
        AnswerStyle::TrueFalse => true_false_marks().to_vec(),
    }
}

fn choice_marks() -> Vec<String> {
    t!("mock.choice_marks")
        .split(',')
        .map(|m| m.trim().to_owned())
        .collect()
}

fn true_false_marks() -> [String; 2] {
    [
        t!("mock.true_mark").into_owned(),
        t!("mock.false_mark").into_owned(),
    ]
}

/// The words for true and false, as answers.
fn true_false_words() -> [String; 2] {
    [
        t!("mock.true_word").into_owned(),
        t!("mock.false_word").into_owned(),
    ]
}

fn answer_label(question: &kg_engine::MockQuestion, answer: u8, marks: &[String]) -> String {
    let index = usize::from(answer);
    if question.options.is_empty() {
        return true_false_words().get(index).cloned().unwrap_or_default();
    }
    format!(
        "({}) {}",
        marks.get(index).map_or("?", String::as_str),
        question.options.get(index).map_or("", String::as_str)
    )
}

fn score_text(result: &MockResult) -> String {
    format!(
        "{}/{} · {}%",
        i18n::points(result.points),
        i18n::points(result.max),
        (result.share() * 100.0).round()
    )
}

fn score_color(ui: &egui::Ui, share: f64) -> egui::Color32 {
    let p = theme::palette(ui.ctx());
    if share >= 0.7 {
        p.success
    } else if share >= 0.5 {
        p.warning
    } else {
        p.danger
    }
}

/// The words of the printed exam, in the app's language.
fn print_request(exam: &MockExam, plan_exam: &str, path: std::path::PathBuf) -> MockPrint {
    let duration = duration_text(exam.format.minutes);
    let mut instructions = vec![
        t!("mock.pdf.rule_time", duration = duration).into_owned(),
        match exam.format.style {
            AnswerStyle::Choice { .. } => t!("mock.pdf.rule_choice"),
            AnswerStyle::TrueFalse => t!("mock.pdf.rule_true_false"),
        }
        .into_owned(),
        t!("mock.pdf.rule_card").into_owned(),
    ];
    if exam.format.wrong_cancels {
        instructions.push(t!("mock.pdf.rule_wrong_cancels").into_owned());
    }
    if exam.proposal.is_some() {
        instructions.push(t!("mock.pdf.rule_essay").into_owned());
    }
    instructions.push(t!("mock.pdf.rule_app").into_owned());
    MockPrint {
        exam: exam.id,
        path,
        subtitle: plan_exam.to_owned(),
        lang: i18n::current(),
        labels: ExamLabels {
            booklet: t!("mock.pdf.booklet").into_owned(),
            duration: t!("mock.pdf.duration").into_owned(),
            questions: t!("mock.pdf.questions").into_owned(),
            code: t!("mock.pdf.code").into_owned(),
            name: t!("mock.pdf.name").into_owned(),
            instructions: t!("mock.pdf.instructions").into_owned(),
            essay: t!("mock.pdf.essay").into_owned(),
            essay_sheet: t!("mock.pdf.essay_sheet").into_owned(),
            answer_card: t!("mock.pdf.answer_card").into_owned(),
            card_hint: t!("mock.pdf.card_hint").into_owned(),
        },
        duration,
        instructions,
        choice_marks: choice_marks(),
        true_false_marks: true_false_marks(),
    }
}
