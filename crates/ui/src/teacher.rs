//! The teacher on the lesson stage: an ink portrait on a paper card.
//!
//! Portraits are "Lorelei" by Lisa Wischofsky (CC0 1.0), built by
//! `tools/teachers.py` as a base drawing plus swappable eyes and mouths.
//! Idle: breathes and blinks. Listening: leans in and nods with the
//! student's voice. Thinking: a thought bubble. Speaking: the mouth follows
//! the loudness of the voice being played. Thoth is the cover engraving and
//! only shows who is speaking through the card's border.

use egui::{Color32, Image, Rect, Stroke, Vec2, pos2, vec2};
use kg_engine::{LessonPhase, Teacher};

use crate::art::Art;
use crate::theme;

/// What drives the animation this frame.
#[derive(Debug, Clone, Copy)]
pub struct Mood {
    pub phase: LessonPhase,
    /// Tutor voice loudness, `0.0..=1.0`.
    pub speech: f32,
    /// Student microphone level, `0.0..=1.0`.
    pub mic: f32,
}

/// Paper of the portrait card, the same in both themes: it is a print.
const PAPER: Color32 = Color32::from_rgb(0xF6, 0xF3, 0xEC);
const PRINT_INK: Color32 = Color32::from_rgb(0x1C, 0x1B, 0x19);
const MOUTHS: usize = 4;

struct Parts {
    base: &'static str,
    eyes: [&'static str; 2],
    mouths: [&'static str; MOUTHS],
}

macro_rules! parts {
    ($name:literal) => {
        Parts {
            base: include_str!(concat!("../assets/teachers/", $name, "/base.svg")),
            eyes: [
                include_str!(concat!("../assets/teachers/", $name, "/eyes-open.svg")),
                include_str!(concat!("../assets/teachers/", $name, "/eyes-closed.svg")),
            ],
            mouths: [
                include_str!(concat!("../assets/teachers/", $name, "/mouth-0.svg")),
                include_str!(concat!("../assets/teachers/", $name, "/mouth-1.svg")),
                include_str!(concat!("../assets/teachers/", $name, "/mouth-2.svg")),
                include_str!(concat!("../assets/teachers/", $name, "/mouth-3.svg")),
            ],
        }
    };
}

/// Drawn portraits; `None` for Thoth, who uses the engraving.
fn parts(teacher: Teacher) -> Option<Parts> {
    Some(match teacher {
        Teacher::Helena => parts!("helena"),
        Teacher::Lia => parts!("lia"),
        Teacher::Rafael => parts!("rafael"),
        Teacher::Otavio => parts!("otavio"),
        Teacher::Thoth => return None,
    })
}

fn slug(teacher: Teacher) -> &'static str {
    match teacher {
        Teacher::Helena => "helena",
        Teacher::Lia => "lia",
        Teacher::Rafael => "rafael",
        Teacher::Otavio => "otavio",
        Teacher::Thoth => "thoth",
    }
}

fn uri(teacher: Teacher, mouth: usize, eyes_closed: bool) -> String {
    format!(
        "bytes://teacher-{}-m{mouth}-e{}.svg",
        slug(teacher),
        u8::from(eyes_closed)
    )
}

/// Makes every pose of `teacher` loadable by URI, once per context.
fn register(ctx: &egui::Context, teacher: Teacher) {
    let id = egui::Id::new(("teacher_registered", slug(teacher)));
    if ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
        return;
    }
    if let Some(parts) = parts(teacher) {
        for (mouth, mouth_svg) in parts.mouths.iter().enumerate() {
            for (closed, eyes_svg) in parts.eyes.iter().enumerate() {
                let svg = parts
                    .base
                    .replacen("<!--eyes-->", eyes_svg.trim(), 1)
                    .replacen("<!--mouth-->", mouth_svg.trim(), 1);
                ctx.include_bytes(uri(teacher, mouth, closed == 1), svg.into_bytes());
            }
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, true));
}

/// The teacher from the settings.
pub fn current(engine: &kg_engine::Engine) -> Teacher {
    engine.settings().teacher()
}

/// A still portrait (resting face), for pickers.
pub fn still(ctx: &egui::Context, teacher: Teacher, size: f32) -> Image<'static> {
    if teacher == Teacher::Thoth {
        return Art::Head.image(size);
    }
    register(ctx, teacher);
    Image::new(uri(teacher, 0, false))
        .fit_to_exact_size(Vec2::splat(size))
        .bg_fill(PAPER)
}

/// Paints the teacher's card centered in `rect`.
pub fn paint(ui: &mut egui::Ui, rect: Rect, mood: Mood, teacher: Teacher) {
    let p = theme::palette(ui.ctx());
    let t = ui.input(|i| i.time) as f32;
    let side = rect.width().min(rect.height()) * 0.86;
    let s = side / 300.0;
    let plate = Rect::from_center_size(rect.center() + vec2(0.0, 8.0 * s), Vec2::splat(side));

    // Mouth follows the voice smoothly; keyed by position so several
    // teachers on screen never share a mouth.
    let mouth_id = ui
        .id()
        .with(("teacher_mouth", rect.min.x as i32, rect.min.y as i32));
    let target = if mood.phase == LessonPhase::Speaking {
        mood.speech
    } else {
        0.0
    };
    let open = ui.ctx().animate_value_with_time(mouth_id, target, 0.07);

    let painter = ui.painter().clone();
    // Listening: a ring that swells with the student's voice.
    if mood.phase == LessonPhase::Listening {
        painter.rect_stroke(
            plate.expand(6.0 + mood.mic.min(1.0) * 8.0 * s),
            0.0,
            Stroke::new(2.0, p.accent.gamma_multiply(0.35 + mood.mic.min(1.0) * 0.5)),
            egui::StrokeKind::Outside,
        );
    }
    // Hard offset shadow, as on the other cards.
    painter.rect_filled(plate.translate(vec2(6.0, 6.0) * s.max(0.6)), 0.0, p.shadow);
    painter.rect_filled(plate, 0.0, PAPER);

    // Motion: breathing always; nodding while listening; a lean per phase.
    let breath = (t * 1.3).sin() * 1.6 * s;
    let nod = if mood.phase == LessonPhase::Listening {
        (t * 4.0).sin() * 2.5 * s * (0.3 + mood.mic)
    } else {
        0.0
    };
    let lean = match mood.phase {
        LessonPhase::Listening => 0.035,
        LessonPhase::Thinking | LessonPhase::Transcribing => -0.025,
        _ => (t * 0.5).sin() * 0.012,
    };

    let mut clip = ui.new_child(egui::UiBuilder::new().max_rect(plate));
    clip.set_clip_rect(plate.shrink(1.0));
    match parts(teacher) {
        Some(_) => {
            register(ui.ctx(), teacher);
            let mouth = match open {
                o if o < 0.06 => 0,
                o if o < 0.22 => 1,
                o if o < 0.45 => 2,
                _ => 3,
            };
            // Blink every few seconds, each teacher at their own pace.
            let period = 4.1 + teacher as u8 as f32 * 0.37;
            let blinking = (t % period) < 0.12;
            // The drawing reaches its bottom edge (shoulders), so it sits on
            // the card's bottom and moves up and down from there.
            // Slightly larger than the card, so the face fills it; the
            // shoulders are cut by the card's bottom edge.
            let size = side * 1.08;
            let image = Rect::from_min_size(
                pos2(
                    plate.center().x - size / 2.0,
                    plate.bottom() - size * 0.99 + breath + nod,
                ),
                Vec2::splat(size),
            );
            Image::new(uri(teacher, mouth, blinking))
                .rotate(lean, Vec2::new(0.5, 0.9))
                .paint_at(&clip, image);
        }
        None => {
            let grow = 1.0 + open.min(1.0) * 0.015;
            let image = Rect::from_center_size(
                plate.center() + vec2(0.0, breath + nod),
                Vec2::splat(side * 0.94 * grow),
            );
            Art::Head
                .image(side)
                .rotate(lean * 0.6, Vec2::new(0.5, 0.8))
                .paint_at(&clip, image);
        }
    }

    // Who has the floor: a heavy accent border while speaking.
    let (width, color) = if mood.phase == LessonPhase::Speaking {
        ((3.0 + open.min(1.0) * 2.0) * s.max(0.6), p.accent)
    } else {
        (1.6 * s.max(0.6), PRINT_INK)
    };
    painter.rect_stroke(
        plate,
        0.0,
        Stroke::new(width, color),
        egui::StrokeKind::Inside,
    );

    if matches!(
        mood.phase,
        LessonPhase::Thinking | LessonPhase::Transcribing
    ) {
        thought_bubble(
            &painter,
            plate.right_top() + vec2(-18.0, 26.0) * s,
            s,
            t,
            p.accent,
        );
    }
}

/// Three dots in a bubble, at the top right of the card.
fn thought_bubble(painter: &egui::Painter, anchor: egui::Pos2, s: f32, t: f32, accent: Color32) {
    let fill = PAPER;
    let ink = Stroke::new(1.6 * s.max(0.6), PRINT_INK);
    let bubble = Rect::from_center_size(anchor + vec2(0.0, -14.0) * s, vec2(62.0, 32.0) * s);
    for (offset, radius) in [(vec2(-30.0, 22.0), 4.0), (vec2(-20.0, 10.0), 6.0)] {
        let center = anchor + offset * s;
        painter.circle_filled(center, radius * s, fill);
        painter.circle_stroke(center, radius * s, ink);
    }
    painter.rect_filled(bubble, 0.0, fill);
    painter.rect_stroke(bubble, 0.0, ink, egui::StrokeKind::Inside);
    for i in 0..3 {
        let phase = (t * 3.0 - i as f32 * 0.6).sin() * 0.5 + 0.5;
        painter.circle_filled(
            bubble.center() + vec2((i as f32 - 1.0) * 15.0, 0.0) * s,
            (3.0 + phase * 1.6) * s,
            if phase > 0.7 { accent } else { PRINT_INK },
        );
    }
}
