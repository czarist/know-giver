//! Voice lesson as a meeting: the teacher on stage with captions, a
//! whiteboard with key points, the student's tile with the mic level, and a
//! control bar. The transcript and typed chat open in a side panel.

use egui::{Color32, RichText, Stroke, pos2, vec2};
use kg_engine::{Command, Engine, LessonPhase, LessonSession, Speaker, TopicStatus};
use rust_i18n::t;

use super::{Actions, current_week};
use crate::app::{KnowGiverApp, Screen, UiState};
use crate::icons::Icon;
use crate::{avatar, i18n, theme, widgets};

/// Height of the control bar under the stage.
const CONTROLS_HEIGHT: f32 = 78.0;
/// Width of the transcript / chat panel.
const PANEL_WIDTH: f32 = 330.0;
/// Characters per second the whiteboard "writes".
const BOARD_WRITE_SPEED: f32 = 28.0;
/// Board colors: a slate green board with chalk-like text, in both themes.
const BOARD_FILL: Color32 = Color32::from_rgb(0x24, 0x38, 0x33);
const BOARD_INK: Color32 = Color32::from_rgb(0xEA, 0xE6, 0xD9);

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    let state = &mut app.ui;

    let Some(lesson) = engine.lesson() else {
        no_lesson(ui, engine, &mut actions);
        actions.run(app);
        return;
    };
    // Keep the teacher alive (breathing, blinking) even when nothing happens.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(33));
    keyboard(ui, lesson, &mut actions);

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(&lesson.topic_title)
                    .family(theme::heading_family())
                    .size(22.0),
            );
            widgets::hint(ui, &lesson.subject);
        });
    });
    model_warnings(ui, engine, &mut actions);

    if state.lesson_panel {
        egui::Panel::right("lesson_panel")
            .resizable(false)
            .exact_size(PANEL_WIDTH)
            .frame(egui::Frame::new().inner_margin(egui::Margin {
                left: 14,
                ..Default::default()
            }))
            .show(ui, |ui| {
                side_panel(ui, lesson, &mut state.lesson_input, &mut actions)
            });
    }

    let stage_height = (ui.available_height() - CONTROLS_HEIGHT).max(260.0);
    let (stage, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), stage_height),
        egui::Sense::hover(),
    );
    paint_stage(ui, stage, engine, lesson, state.lesson_captions);
    controls(ui, engine, lesson, state, &mut actions);
    actions.run(app);
}

/// Space (held) to talk, Esc to interrupt, unless a text field has focus.
fn keyboard(ui: &egui::Ui, lesson: &LessonSession, actions: &mut Actions) {
    if ui.ctx().memory(|m| m.focused().is_some()) {
        return;
    }
    let (pressed, released, escape) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::Space),
            i.key_released(egui::Key::Space),
            i.key_pressed(egui::Key::Escape),
        )
    });
    match lesson.phase {
        LessonPhase::Idle | LessonPhase::Speaking if pressed => {
            actions.send(Command::StartListening);
        }
        LessonPhase::Listening if released => actions.send(Command::StopListening),
        LessonPhase::Speaking if escape => actions.send(Command::StopSpeaking),
        _ => {}
    }
}

fn paint_stage(
    ui: &mut egui::Ui,
    stage: egui::Rect,
    engine: &Engine,
    lesson: &LessonSession,
    captions: bool,
) {
    let p = theme::palette(ui.ctx());
    let painter = ui.painter_at(stage);
    let stage_fill = if ui.visuals().dark_mode {
        Color32::from_rgb(0x17, 0x19, 0x1B)
    } else {
        Color32::from_rgb(0xE9, 0xE6, 0xDF)
    };
    painter.rect_filled(stage, 14.0, stage_fill);

    // Layout: teacher on the left, whiteboard on the right when it fits.
    let gap = 16.0;
    let has_board = stage.width() > 640.0;
    let teacher_width = if has_board {
        stage.width() * 0.52
    } else {
        stage.width()
    };
    let teacher_area = egui::Rect::from_min_size(stage.min, vec2(teacher_width, stage.height()));
    let figure = egui::Rect::from_center_size(
        teacher_area.center() - vec2(0.0, 18.0),
        vec2(
            teacher_area.height().min(teacher_width),
            teacher_area.height() * 0.86,
        ),
    );
    avatar::paint(
        ui,
        figure.intersect(teacher_area.shrink(4.0)),
        avatar::Mood {
            phase: lesson.phase,
            speech: engine.speech_level(),
            mic: lesson.input_level(),
        },
        p.accent,
        avatar::appearance(engine),
    );

    // Name tag, bottom left, like a video call.
    let tag = egui::Rect::from_min_size(
        teacher_area.left_bottom() + vec2(14.0, -40.0),
        vec2(170.0, 28.0),
    );
    painter.rect_filled(tag, 8.0, Color32::from_black_alpha(150));
    painter.text(
        tag.left_center() + vec2(12.0, 0.0),
        egui::Align2::LEFT_CENTER,
        format!("{}  ·  {}", t!("lesson.tutor"), i18n::phase(lesson.phase)),
        egui::FontId::proportional(13.0),
        Color32::WHITE,
    );

    if captions {
        paint_captions(ui, teacher_area, lesson);
    }
    if has_board {
        let board = egui::Rect::from_min_max(
            pos2(teacher_area.right() + gap, stage.top() + gap),
            pos2(stage.right() - gap, stage.bottom() - 150.0),
        );
        paint_board(ui, board, lesson);
        let tile = egui::Rect::from_min_max(
            pos2(board.left(), board.bottom() + gap),
            pos2(board.right(), stage.bottom() - gap),
        );
        paint_student(ui, tile, lesson);
    } else {
        let tile =
            egui::Rect::from_min_size(stage.right_top() + vec2(-196.0, 16.0), vec2(180.0, 110.0));
        paint_student(ui, tile, lesson);
    }
}

/// The latest line of the conversation, over the bottom of the teacher area.
fn paint_captions(ui: &egui::Ui, area: egui::Rect, lesson: &LessonSession) {
    let Some(turn) = lesson.turns.last() else {
        return;
    };
    let prefix = match turn.speaker {
        Speaker::Tutor => String::new(),
        Speaker::Student => format!("{}: ", t!("lesson.you")),
    };
    let text = format!("{prefix}{}", turn.text);
    let width = (area.width() - 80.0).max(200.0);
    let galley = ui.painter().layout(
        last_lines(&text, 220),
        egui::FontId::proportional(16.0),
        Color32::WHITE,
        width,
    );
    let size = galley.size() + vec2(28.0, 18.0);
    let rect = egui::Rect::from_center_size(
        pos2(area.center().x, area.bottom() - 56.0 - size.y / 2.0),
        size,
    );
    let painter = ui.painter_at(area);
    painter.rect_filled(rect, 10.0, Color32::from_black_alpha(165));
    painter.galley(rect.min + vec2(14.0, 9.0), galley, Color32::WHITE);
}

/// The end of a long text, so captions show what is being said last.
fn last_lines(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_owned();
    }
    let tail: String = text.chars().skip(count - max_chars).collect();
    let start = tail.find(' ').map_or(0, |i| i + 1);
    format!("…{}", &tail[start..])
}

/// Key points written on the whiteboard; the newest appears letter by letter.
fn paint_board(ui: &egui::Ui, board: egui::Rect, lesson: &LessonSession) {
    let painter = ui.painter_at(board);
    painter.rect_filled(board, 12.0, BOARD_FILL);
    painter.rect_stroke(
        board,
        12.0,
        Stroke::new(6.0, Color32::from_rgb(0x8A, 0x6A, 0x4A)),
        egui::StrokeKind::Inside,
    );
    painter.text(
        board.left_top() + vec2(22.0, 18.0),
        egui::Align2::LEFT_TOP,
        t!("lesson.board").to_uppercase(),
        egui::FontId::proportional(11.5),
        BOARD_INK.gamma_multiply(0.6),
    );

    let now = ui.input(|i| i.time);
    let width = board.width() - 52.0;
    let mut y = board.top() + 46.0;
    let newest = lesson.board.len().saturating_sub(1);
    for (i, point) in lesson.board.iter().enumerate().rev().take(3) {
        // Remember when each point first appeared, to animate its writing.
        let id = ui.id().with(("board_point", lesson.id, i, point.as_str()));
        let first_seen = ui
            .ctx()
            .data_mut(|d| *d.get_temp_mut_or_insert_with(id, || now));
        let shown = ((now - first_seen) as f32 * BOARD_WRITE_SPEED) as usize;
        let entry = crate::whiteboard::parse(point);
        if !matches!(entry, crate::whiteboard::Entry::Text(_)) {
            let area = egui::Rect::from_min_max(
                pos2(board.left() + 24.0, y),
                pos2(board.right() - 24.0, board.bottom() - 14.0),
            );
            if area.height() <= 20.0 {
                break;
            }
            let opacity = ((now - first_seen) as f32 / 0.25).clamp(0.0, 1.0);
            y += crate::whiteboard::paint(
                &painter.with_clip_rect(area),
                area,
                &entry,
                BOARD_INK.gamma_multiply(opacity),
            );
            continue;
        }
        let text: String = point.chars().take(shown.max(1)).collect();
        let alpha = if i == newest { 1.0 } else { 0.7 };
        let galley = painter.layout(
            format!("•  {text}"),
            egui::FontId::new(17.0, theme::heading_family()),
            BOARD_INK.gamma_multiply(alpha),
            width,
        );
        if y + galley.size().y > board.bottom() - 12.0 {
            break;
        }
        let height = galley.size().y;
        painter.galley(pos2(board.left() + 24.0, y), galley, BOARD_INK);
        y += height + 12.0;
    }
    if lesson.board.is_empty() {
        painter.text(
            board.center(),
            egui::Align2::CENTER_CENTER,
            t!("lesson.board_empty"),
            egui::FontId::proportional(13.0),
            BOARD_INK.gamma_multiply(0.45),
        );
    }
}

/// The student's tile: name, mic icon and a live level meter.
fn paint_student(ui: &egui::Ui, tile: egui::Rect, lesson: &LessonSession) {
    let p = theme::palette(ui.ctx());
    let painter = ui.painter_at(tile.expand(2.0));
    let listening = lesson.phase == LessonPhase::Listening;
    painter.rect_filled(tile, 12.0, p.surface);
    painter.rect_stroke(
        tile,
        12.0,
        Stroke::new(
            if listening { 2.5 } else { 1.0 },
            if listening { p.accent } else { p.border },
        ),
        egui::StrokeKind::Inside,
    );
    painter.text(
        tile.left_top() + vec2(14.0, 12.0),
        egui::Align2::LEFT_TOP,
        t!("lesson.you"),
        egui::FontId::new(14.0, theme::strong_family()),
        p.text,
    );
    let status = if listening {
        t!("lesson.phase.listening")
    } else {
        t!("lesson.mic_off")
    };
    painter.text(
        tile.left_top() + vec2(14.0, 32.0),
        egui::Align2::LEFT_TOP,
        status,
        egui::FontId::proportional(12.0),
        p.muted,
    );

    // Level meter: seven bars that follow the microphone.
    let level = lesson.input_level();
    let t = ui.input(|i| i.time) as f32;
    let bars = 7;
    let base = tile.left_bottom() + vec2(14.0, -16.0);
    for i in 0..bars {
        let wobble = ((t * 9.0 + i as f32 * 1.3).sin() * 0.5 + 0.5) * 0.6 + 0.4;
        let h = if listening {
            4.0 + 26.0 * (level * wobble).min(1.0)
        } else {
            4.0
        };
        let x = base.x + i as f32 * 9.0;
        painter.rect_filled(
            egui::Rect::from_min_max(pos2(x, base.y - h), pos2(x + 5.0, base.y)),
            2.5,
            if listening { p.accent } else { p.border },
        );
    }
}

fn controls(
    ui: &mut egui::Ui,
    engine: &Engine,
    lesson: &LessonSession,
    state: &mut UiState,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    let busy = matches!(
        lesson.phase,
        LessonPhase::Thinking | LessonPhase::Transcribing
    );
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        // Center the round buttons.
        let group_width = 5.0 * 52.0 + 130.0;
        ui.add_space(((ui.available_width() - group_width) / 2.0).max(0.0));

        let can_talk = engine.stt_ready() && !busy;
        let mic_fill = if lesson.phase == LessonPhase::Listening {
            p.highlight
        } else {
            p.accent
        };
        let mic = round_button(ui, Icon::Mic, Color32::WHITE, mic_fill, 56.0, can_talk)
            .on_hover_text(t!("lesson.hint_talk"));
        if mic.clicked() {
            match lesson.phase {
                LessonPhase::Idle | LessonPhase::Speaking => actions.send(Command::StartListening),
                LessonPhase::Listening => actions.send(Command::StopListening),
                LessonPhase::Thinking | LessonPhase::Transcribing => {}
            }
        }
        let speaking = lesson.phase == LessonPhase::Speaking;
        if round_button(ui, Icon::Stop, p.text, p.surface, 46.0, speaking)
            .on_hover_text(t!("lesson.interrupt"))
            .clicked()
        {
            actions.send(Command::StopSpeaking);
        }
        let cc = if state.lesson_captions {
            p.accent
        } else {
            p.muted
        };
        if round_button(ui, Icon::Sparkles, cc, p.surface, 46.0, true)
            .on_hover_text(t!("lesson.captions"))
            .clicked()
        {
            state.lesson_captions = !state.lesson_captions;
        }
        let chat = if state.lesson_panel {
            p.accent
        } else {
            p.muted
        };
        if round_button(ui, Icon::Edital, chat, p.surface, 46.0, true)
            .on_hover_text(t!("lesson.transcript"))
            .clicked()
        {
            state.lesson_panel = !state.lesson_panel;
        }
        ui.add_space(12.0);
        let end = egui::Button::new(
            RichText::new(t!("lesson.end"))
                .color(Color32::WHITE)
                .family(theme::strong_family()),
        )
        .fill(p.danger)
        .corner_radius(23.0)
        .min_size(vec2(120.0, 46.0));
        if ui.add(end).clicked() {
            actions.send(Command::EndLesson);
        }
    });
    ui.vertical_centered(|ui| {
        widgets::hint(ui, &t!("lesson.keys"));
    });
}

fn round_button(
    ui: &mut egui::Ui,
    icon: Icon,
    tint: Color32,
    fill: Color32,
    size: f32,
    enabled: bool,
) -> egui::Response {
    let button = egui::Button::image(icon.image(size * 0.42, tint))
        .fill(fill)
        .corner_radius(size / 2.0)
        .min_size(vec2(size, size));
    ui.add_enabled(enabled, button)
}

/// Transcript of the lesson and the typed chat.
fn side_panel(
    ui: &mut egui::Ui,
    lesson: &LessonSession,
    input: &mut String,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    ui.label(RichText::new(t!("lesson.transcript")).family(theme::strong_family()));
    if !lesson.board.is_empty() {
        egui::CollapsingHeader::new(t!("lesson.board"))
            .id_salt("board_transcript")
            .show(ui, |ui| {
                for entry in &lesson.board {
                    match crate::whiteboard::parse(entry) {
                        crate::whiteboard::Entry::Formula(text)
                        | crate::whiteboard::Entry::Text(text) => {
                            ui.label(text);
                        }
                        crate::whiteboard::Entry::Diagram(nodes) => {
                            ui.label(nodes.join(" → "));
                        }
                    }
                }
            });
    }
    let busy = matches!(
        lesson.phase,
        LessonPhase::Thinking | LessonPhase::Transcribing
    );
    let input_height = 46.0;
    egui::ScrollArea::vertical()
        .max_height((ui.available_height() - input_height).max(100.0))
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for turn in &lesson.turns {
                let (who, color) = match turn.speaker {
                    Speaker::Tutor => (t!("lesson.tutor"), p.accent),
                    Speaker::Student => (t!("lesson.you"), p.highlight),
                };
                ui.label(RichText::new(who).small().color(color));
                ui.label(&turn.text);
                ui.add_space(4.0);
            }
            if busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    widgets::hint(ui, &i18n::phase(lesson.phase));
                });
            }
        });
    ui.horizontal(|ui| {
        let field = ui.add_enabled(
            !busy,
            egui::TextEdit::singleline(input)
                .hint_text(t!("lesson.type_hint"))
                .desired_width((ui.available_width() - 80.0).max(120.0)),
        );
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let send = ui
            .add_enabled(
                !busy && !input.trim().is_empty(),
                egui::Button::new(t!("lesson.send")),
            )
            .clicked();
        if (enter || send) && !input.trim().is_empty() {
            actions.send(Command::SendMessage(std::mem::take(input)));
            field.request_focus();
        }
    });
}

fn no_lesson(ui: &mut egui::Ui, engine: &Engine, actions: &mut Actions) {
    widgets::header(ui, &t!("lesson.title"), &t!("lesson.subtitle"));
    let Some(plan) = engine.active_plan() else {
        widgets::hint(ui, &t!("plan.empty"));
        if ui.button(t!("plan.go_to_edital")).clicked() {
            actions.go_to(Screen::Edital);
        }
        return;
    };
    widgets::hint(ui, &t!("lesson.none"));
    ui.add_space(8.0);
    ui.label(RichText::new(t!("lesson.suggestions")).family(theme::strong_family()));
    let week = current_week(plan);
    let suggestions = plan
        .topics()
        .filter(|(_, t)| t.status != TopicStatus::Done && t.week <= week)
        .take(6);
    let p = theme::palette(ui.ctx());
    for (subject, topic) in suggestions {
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(&topic.title);
                    widgets::hint(ui, &subject.name);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let start = egui::Button::image_and_text(
                        Icon::Play.image(16.0, Color32::WHITE),
                        RichText::new(t!("lesson.start")).color(Color32::WHITE),
                    )
                    .fill(p.accent);
                    if ui.add(start).clicked() {
                        actions.send(Command::StartLesson(topic.id));
                    }
                });
            });
        });
    }
}

fn model_warnings(ui: &mut egui::Ui, engine: &Engine, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    let mut warn = |ui: &mut egui::Ui, text: &str| {
        widgets::callout(ui, p.warning, |ui| {
            ui.horizontal(|ui| {
                ui.label(text);
                if ui.small_button(t!("lesson.open_settings")).clicked() {
                    actions.go_to(Screen::Settings);
                }
            });
        });
    };
    if !engine.stt_ready() {
        warn(ui, &t!("lesson.need_stt"));
    }
    if engine.settings().voice_enabled && !engine.tts_ready() {
        warn(ui, &t!("lesson.need_tts"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captions_keep_the_end_of_long_text() {
        assert_eq!(last_lines("curto", 10), "curto");
        let long = "uma frase bem comprida que não cabe";
        let tail = last_lines(long, 12);
        assert!(tail.starts_with('…'));
        assert!(tail.ends_with("cabe"));
    }
}
