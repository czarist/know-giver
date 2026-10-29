//! Voice lesson as a meeting: the teacher on stage with captions, a
//! whiteboard with key points, the student's tile with the mic level, and a
//! control bar. The transcript and typed chat open in a side panel.

use egui::{Color32, RichText, Stroke, pos2, vec2};
use kg_engine::{Command, Engine, LessonPhase, LessonRecap, LessonSession, Speaker, TopicId};
use rust_i18n::t;

use super::{Actions, current_week};
use crate::app::{KnowGiverApp, Screen, UiState};
use crate::icons::Icon;
use crate::{i18n, teacher, theme, widgets};

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
        match engine.recap() {
            Some((topic_id, recap)) => recap_view(ui, engine, topic_id, recap, &mut actions),
            None => no_lesson(ui, engine, &mut actions),
        }
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
    progress(ui, lesson);
    if lesson.finished {
        finished_card(ui, lesson, &mut actions);
    }
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

/// Name of a step of the lesson plan; the last one is the final review.
fn step_name(lesson: &LessonSession, step: usize) -> String {
    lesson
        .outline
        .get(step)
        .cloned()
        .unwrap_or_else(|| t!("lesson.final_review").into_owned())
}

/// Where the lesson is: "Step 2 of 4 · name" over a bar with one segment per
/// step. Hovering shows the whole plan.
fn progress(ui: &mut egui::Ui, lesson: &LessonSession) {
    let p = theme::palette(ui.ctx());
    ui.add_space(6.0);
    if lesson.outline.is_empty() && !lesson.finished {
        widgets::hint(ui, &t!("lesson.plan_building"));
        ui.add_space(4.0);
        return;
    }
    let total = lesson.total_steps();
    let current = lesson.step.min(total - 1);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t!("lesson.step_of", step = current + 1, total = total))
                .family(theme::strong_family())
                .color(p.accent),
        );
        ui.label(RichText::new(step_name(lesson, current)).color(p.muted));
    });
    let (bar, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 8.0), egui::Sense::hover());
    let gap = 4.0;
    let width = (bar.width() - gap * (total - 1) as f32) / total as f32;
    for i in 0..total {
        let rect = egui::Rect::from_min_size(
            pos2(bar.left() + i as f32 * (width + gap), bar.top()),
            vec2(width, bar.height()),
        );
        let done = i < current || lesson.finished;
        let fill = if done {
            p.accent
        } else if i == current {
            p.highlight
        } else {
            p.border
        };
        ui.painter().rect_filled(rect, 0.0, fill);
    }
    response.on_hover_ui(|ui| outline_list(ui, lesson));
    ui.add_space(6.0);
}

/// The lesson plan as a list, done steps ticked.
fn outline_list(ui: &mut egui::Ui, lesson: &LessonSession) {
    let p = theme::palette(ui.ctx());
    for i in 0..lesson.total_steps() {
        let done = i < lesson.step || lesson.finished;
        let mark = if done {
            "✔"
        } else if i == lesson.step {
            "▶"
        } else {
            "·"
        };
        let color = if done {
            p.muted
        } else if i == lesson.step {
            p.text
        } else {
            p.muted
        };
        ui.label(RichText::new(format!("{mark}  {}", step_name(lesson, i))).color(color));
    }
}

/// Shown when the tutor closed the lesson: take the quiz or end here.
fn finished_card(ui: &mut egui::Ui, lesson: &LessonSession, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(2.0, p.accent))
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(t!("lesson.finished_title"))
                            .family(theme::heading_family())
                            .size(18.0)
                            .color(p.accent),
                    );
                    ui.label(t!("lesson.finished_body"));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t!("lesson.end")).clicked() {
                        actions.send(Command::EndLesson);
                    }
                    let quiz = egui::Button::new(
                        RichText::new(t!("lesson.take_quiz")).color(Color32::WHITE),
                    )
                    .fill(p.accent);
                    if ui.add(quiz).clicked() {
                        actions.send(Command::EndLesson);
                        actions.send(Command::StartQuiz(lesson.topic_id));
                        actions.go_to(Screen::Quiz);
                    }
                });
            });
        });
    ui.add_space(6.0);
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
    let painter = ui.painter_at(stage);
    let stage_fill = if ui.visuals().dark_mode {
        Color32::from_rgb(0x17, 0x19, 0x1B)
    } else {
        Color32::from_rgb(0xE9, 0xE6, 0xDF)
    };
    painter.rect_filled(stage, 0.0, stage_fill);

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
    let who = teacher::current(engine);
    teacher::paint(
        ui,
        figure.intersect(teacher_area.shrink(4.0)),
        teacher::Mood {
            phase: lesson.phase,
            speech: engine.speech_level(),
            mic: lesson.input_level(),
        },
        who,
    );

    // Name tag, bottom left, like a video call.
    let tag = egui::Rect::from_min_size(
        teacher_area.left_bottom() + vec2(14.0, -40.0),
        vec2(170.0, 28.0),
    );
    painter.rect_filled(tag, 0.0, Color32::from_black_alpha(150));
    painter.text(
        tag.left_center() + vec2(12.0, 0.0),
        egui::Align2::LEFT_CENTER,
        format!(
            "{}  ·  {}",
            i18n::teacher_name(who),
            i18n::phase(lesson.phase)
        ),
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
        paint_board(ui, board, engine, lesson);
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
    let text = if let Some(live) = lesson.live_caption() {
        // What the student is saying, as they say it.
        format!("{}: {live}…", t!("lesson.you"))
    } else {
        let Some(turn) = lesson.turns.last() else {
            return;
        };
        let prefix = match turn.speaker {
            Speaker::Tutor => String::new(),
            Speaker::Student => format!("{}: ", t!("lesson.you")),
        };
        format!("{prefix}{}", turn.text)
    };
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
    painter.rect_filled(rect, 0.0, Color32::from_black_alpha(165));
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

/// What the tutor wrote on the whiteboard, newest on top. Key points appear
/// letter by letter; tables, maps and pictures fade in.
fn paint_board(ui: &egui::Ui, board: egui::Rect, engine: &Engine, lesson: &LessonSession) {
    let painter = ui.painter_at(board);
    painter.rect_filled(board, 0.0, BOARD_FILL);
    painter.rect_stroke(
        board,
        0.0,
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
    let mut drawn = 0;
    for (i, point) in lesson.board.iter().enumerate().rev().take(5) {
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
            if area.height() <= 40.0 {
                break;
            }
            let picture = board_picture(ui.ctx(), engine, &entry);
            let opacity = ((now - first_seen) as f32 / 0.25).clamp(0.0, 1.0);
            let draw = |clip: egui::Rect| {
                crate::whiteboard::paint(
                    ui,
                    &painter.with_clip_rect(clip),
                    area,
                    &entry,
                    BOARD_INK.gamma_multiply(opacity),
                    picture
                        .as_ref()
                        .map(|(uri, caption)| crate::whiteboard::Picture { uri, caption }),
                )
            };
            // Measure first: only the newest entry may be cut by the board's
            // edge; older ones are left out when they do not fit.
            let height = draw(egui::Rect::NOTHING);
            if drawn > 0 && y + height > board.bottom() - 4.0 {
                break;
            }
            draw(area);
            y += height;
            drawn += usize::from(height > 0.0);
            continue;
        }
        drawn += 1;
        let text: String = point.chars().take(shown.max(1)).collect();
        let alpha = if i == newest { 1.0 } else { 0.7 };
        let galley = painter.layout(
            format!("•  {text}"),
            // Chalk text stays proportional: monospace would not fit the board.
            egui::FontId::new(17.0, theme::strong_family()),
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
    if drawn == 0 {
        painter.text(
            board.center(),
            egui::Align2::CENTER_CENTER,
            t!("lesson.board_empty"),
            egui::FontId::proportional(13.0),
            BOARD_INK.gamma_multiply(0.45),
        );
    }
}

/// Loader URI and caption of a board picture that finished loading.
fn board_picture(
    ctx: &egui::Context,
    engine: &Engine,
    entry: &crate::whiteboard::Entry<'_>,
) -> Option<(String, String)> {
    let crate::whiteboard::Entry::Image(query) = entry else {
        return None;
    };
    let Some(kg_engine::Illustration::Ready(image)) = engine.illustration(query) else {
        return None;
    };
    let key = kg_engine::illustration_key(query);
    let uri = format!(
        "bytes://board-{:x}.{}",
        key.bytes()
            .fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b))),
        image.format
    );
    let id = egui::Id::new(("board_picture", uri.as_str()));
    if !ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
        ctx.include_bytes(uri.clone(), image.bytes.clone());
        ctx.data_mut(|d| d.insert_temp(id, true));
    }
    Some((uri, image.title.clone()))
}

/// The student's tile: name, mic icon and a live level meter.
fn paint_student(ui: &egui::Ui, tile: egui::Rect, lesson: &LessonSession) {
    let p = theme::palette(ui.ctx());
    let painter = ui.painter_at(tile.expand(2.0));
    let listening = lesson.phase == LessonPhase::Listening;
    painter.rect_filled(tile, 0.0, p.surface);
    painter.rect_stroke(
        tile,
        0.0,
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
        .corner_radius(0.0)
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
    if !lesson.outline.is_empty() {
        egui::CollapsingHeader::new(t!("lesson.outline"))
            .id_salt("lesson_outline")
            .default_open(true)
            .show(ui, |ui| outline_list(ui, lesson));
    }
    if !lesson.board.is_empty() {
        egui::CollapsingHeader::new(t!("lesson.board"))
            .id_salt("board_transcript")
            .show(ui, |ui| {
                for entry in &lesson.board {
                    if let Some(text) = crate::whiteboard::parse(entry).summary() {
                        ui.label(text);
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

/// A taken lesson, looked back at: its plan, the board and the closing
/// words, with the way to take it again.
fn recap_view(
    ui: &mut egui::Ui,
    engine: &Engine,
    topic_id: TopicId,
    recap: Option<&LessonRecap>,
    actions: &mut Actions,
) {
    let p = theme::palette(ui.ctx());
    let found = engine
        .active_plan()
        .and_then(|plan| plan.find_topic(topic_id));
    let (subject, title) = found.map_or((String::new(), String::new()), |(s, t)| {
        (s.name.clone(), t.title.clone())
    });
    widgets::header(ui, &title, &subject);
    ui.horizontal(|ui| {
        if widgets::primary_button(ui, &t!("lesson.retake")).clicked() {
            actions.send(Command::StartLesson(topic_id));
        }
        if ui.button(t!("plan.quiz")).clicked() {
            actions.send(Command::CloseRecap);
            actions.send(Command::StartQuiz(topic_id));
            actions.go_to(Screen::Quiz);
        }
        let mistakes = engine
            .mistakes()
            .iter()
            .filter(|m| m.topic_id == topic_id)
            .count();
        if mistakes > 0
            && ui
                .button(t!("quiz.retry_mistakes", count = mistakes))
                .clicked()
        {
            actions.send(Command::CloseRecap);
            actions.send(Command::RetryMistakes(topic_id));
            actions.go_to(Screen::Quiz);
        }
        if ui.button(t!("common.back")).clicked() {
            actions.send(Command::CloseRecap);
        }
    });
    ui.add_space(10.0);
    let Some(recap) = recap
        .filter(|r| !(r.outline.is_empty() && r.board.is_empty() && r.summary.trim().is_empty()))
    else {
        widgets::hint(ui, &t!("lesson.recap_missing"));
        return;
    };
    egui::ScrollArea::vertical().show(ui, |ui| {
        if let Some(day) = recap.taken_on {
            widgets::hint(ui, &t!("lesson.recap_taken_on", date = i18n::date(day)));
            ui.add_space(6.0);
        }
        if !recap.summary.trim().is_empty() {
            widgets::card(ui, |ui| {
                ui.label(
                    RichText::new(t!("lesson.recap_summary"))
                        .small()
                        .color(p.muted),
                );
                ui.label(RichText::new(recap.summary.trim()).size(16.0));
            });
        }
        if !recap.outline.is_empty() {
            widgets::card(ui, |ui| {
                ui.label(RichText::new(t!("lesson.outline")).small().color(p.muted));
                for (i, step) in recap.outline.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add(Icon::Check.image(14.0, p.success));
                        ui.label(format!("{}. {step}", i + 1));
                    });
                }
            });
        }
        if !recap.board.is_empty() {
            ui.add_space(4.0);
            recap_board(ui, &recap.board);
        }
    });
}

/// The lesson's board, all of it, in chalk.
fn recap_board(ui: &mut egui::Ui, entries: &[String]) {
    egui::Frame::new()
        .fill(BOARD_FILL)
        .stroke(Stroke::new(6.0, Color32::from_rgb(0x8A, 0x6A, 0x4A)))
        .inner_margin(22)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(t!("lesson.board").to_uppercase())
                    .size(11.5)
                    .color(BOARD_INK.gamma_multiply(0.6)),
            );
            ui.add_space(8.0);
            for point in entries {
                let entry = crate::whiteboard::parse(point);
                if matches!(entry, crate::whiteboard::Entry::Image(_)) {
                    continue;
                }
                let width = ui.available_width();
                let probe = egui::Rect::from_min_size(ui.cursor().min, vec2(width, 2000.0));
                let painter = ui.painter().clone();
                let height = crate::whiteboard::paint(
                    ui,
                    &painter.with_clip_rect(egui::Rect::NOTHING),
                    probe,
                    &entry,
                    BOARD_INK,
                    None,
                );
                let (rect, _) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
                crate::whiteboard::paint(ui, &painter, rect, &entry, BOARD_INK, None);
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
    // Unfinished lessons first, then lessons not taken yet.
    let unfinished = plan.topics().filter(|(_, t)| engine.has_lesson_draft(t.id));
    let suggestions =
        unfinished
            .chain(plan.topics().filter(|(_, t)| {
                !t.lesson_taken() && t.week <= week && !engine.has_lesson_draft(t.id)
            }))
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
                    let resume = engine.has_lesson_draft(topic.id);
                    let (label, command) = if resume {
                        (t!("lesson.continue"), Command::ResumeLesson(topic.id))
                    } else {
                        (t!("lesson.start"), Command::StartLesson(topic.id))
                    };
                    let start = egui::Button::image_and_text(
                        Icon::Play.image(16.0, Color32::WHITE),
                        RichText::new(label).color(Color32::WHITE),
                    )
                    .fill(p.accent);
                    if ui.add(start).clicked() {
                        actions.send(command);
                    }
                    if resume && ui.button(t!("lesson.start_over")).clicked() {
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
