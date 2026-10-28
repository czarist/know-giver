//! The animated teacher of the lesson screen, drawn with vector shapes.
//!
//! Idle: breathes and blinks. Listening: looks at the student and nods with
//! their voice. Thinking: looks up, with a small thought bubble. Speaking: the
//! mouth follows the loudness of the voice being played.

use std::f32::consts::{PI, TAU};

use egui::{Color32, Pos2, Shape, Stroke, Vec2, pos2, vec2};
use kg_engine::LessonPhase;

/// What drives the animation this frame.
#[derive(Debug, Clone, Copy)]
pub struct Mood {
    pub phase: LessonPhase,
    /// Tutor voice loudness, `0.0..=1.0`.
    pub speech: f32,
    /// Student microphone level, `0.0..=1.0`.
    pub mic: f32,
}

/// Presentation choices independent from the animation state.
#[derive(Debug, Clone, Copy, Default)]
pub struct Appearance {
    pub female: bool,
    pub minimal: bool,
}

/// Colors of the character; the sweater follows the app accent.
struct Look {
    skin: Color32,
    skin_shade: Color32,
    hair: Color32,
    sweater: Color32,
    shirt: Color32,
    ink: Color32,
    mouth: Color32,
    blush: Color32,
}

fn look(accent: Color32, dark: bool) -> Look {
    Look {
        skin: Color32::from_rgb(0xE8, 0xC2, 0x9E),
        skin_shade: Color32::from_rgb(0xD6, 0xA9, 0x83),
        hair: Color32::from_rgb(0x3A, 0x2E, 0x28),
        sweater: if dark {
            Color32::from_rgb(0x3D, 0x7D, 0x75)
        } else {
            accent
        },
        shirt: Color32::from_rgb(0xF6, 0xF4, 0xEF),
        ink: Color32::from_rgb(0x2B, 0x2A, 0x28),
        mouth: Color32::from_rgb(0x7A, 0x3B, 0x34),
        blush: Color32::from_rgba_unmultiplied(0xE0, 0x8C, 0x7A, 70),
    }
}

/// Paints the teacher centered in `rect`.
pub fn paint(ui: &egui::Ui, rect: egui::Rect, mood: Mood, accent: Color32, appearance: Appearance) {
    let painter = ui.painter_at(rect);
    let t = ui.input(|i| i.time) as f32;
    let s = rect.height() / 320.0;
    let c = look(accent, ui.visuals().dark_mode);
    let center = rect.center();

    // The mouth follows the voice smoothly instead of jumping each frame.
    // Keyed by position, so several avatars on screen never share a mouth.
    let mouth_id = ui
        .id()
        .with(("avatar_mouth", rect.min.x as i32, rect.min.y as i32));
    let target = if mood.phase == LessonPhase::Speaking {
        mood.speech
    } else {
        0.0
    };
    let open = ui.ctx().animate_value_with_time(mouth_id, target, 0.08);

    // Motion: breathing always; nodding while listening.
    let breath = (t * 1.3).sin() * 1.5 * s;
    let nod = if mood.phase == LessonPhase::Listening {
        (t * 4.0).sin() * 3.0 * s * (0.3 + mood.mic)
    } else {
        0.0
    };
    let tilt = match mood.phase {
        LessonPhase::Listening => 0.06,
        LessonPhase::Thinking | LessonPhase::Transcribing => -0.04,
        _ => (t * 0.5).sin() * 0.015,
    };

    // Soft spotlight behind the character.
    painter.circle_filled(
        center + vec2(0.0, -10.0 * s),
        150.0 * s,
        accent.gamma_multiply(0.08),
    );

    // Body: shoulders and sweater, with a V-neck shirt.
    let shoulders_top = center.y + 70.0 * s + breath * 0.5;
    let body = ellipse_top(
        pos2(center.x, rect.bottom() + 40.0 * s),
        125.0 * s,
        rect.bottom() + 40.0 * s - shoulders_top,
    );
    painter.add(Shape::convex_polygon(body, c.sweater, Stroke::NONE));
    let neck_top = center.y + 38.0 * s + breath;
    painter.rect_filled(
        egui::Rect::from_min_max(
            pos2(center.x - 17.0 * s, neck_top),
            pos2(center.x + 17.0 * s, shoulders_top + 12.0 * s),
        ),
        4.0 * s,
        c.skin_shade,
    );
    painter.add(Shape::convex_polygon(
        vec![
            pos2(center.x - 24.0 * s, shoulders_top + 2.0 * s),
            pos2(center.x + 24.0 * s, shoulders_top + 2.0 * s),
            pos2(center.x, shoulders_top + 34.0 * s),
        ],
        c.shirt,
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![
            pos2(center.x - 16.0 * s, shoulders_top + 2.0 * s),
            pos2(center.x + 16.0 * s, shoulders_top + 2.0 * s),
            pos2(center.x, shoulders_top + 22.0 * s),
        ],
        c.skin_shade,
        Stroke::NONE,
    ));

    // Head, rotated slightly around its center.
    let head = center + vec2(0.0, -28.0 * s + breath + nod);
    let rot = |p: Vec2| -> Pos2 {
        let (sin, cos) = tilt.sin_cos();
        head + vec2(p.x * cos - p.y * sin, p.x * sin + p.y * cos) * s
    };

    // Hair back, ears, face.
    if appearance.female {
        painter.rect_filled(
            egui::Rect::from_min_max(rot(vec2(-69.0, -35.0)), rot(vec2(69.0, 83.0))),
            36.0 * s,
            c.hair,
        );
    }
    painter.circle_filled(rot(vec2(0.0, -10.0)), 66.0 * s, c.hair);
    for side in [-1.0, 1.0] {
        painter.circle_filled(rot(vec2(side * 60.0, 6.0)), 11.0 * s, c.skin_shade);
    }
    painter.circle_filled(rot(vec2(0.0, 4.0)), 58.0 * s, c.skin);
    // Fringe: a thick arc of hair across the top of the forehead.
    let fringe: Vec<Pos2> = (0..=24)
        .map(|i| {
            let a = PI * 1.12 + PI * 0.76 * i as f32 / 24.0;
            rot(vec2(a.cos() * 50.0, -6.0 + a.sin() * 44.0))
        })
        .collect();
    painter.add(Shape::line(fringe, Stroke::new(16.0 * s, c.hair)));

    // Eyes: look toward the student while listening, up while thinking.
    let look_at = match mood.phase {
        LessonPhase::Listening => vec2(2.5, 1.0),
        LessonPhase::Thinking | LessonPhase::Transcribing => vec2(2.0, -3.5),
        _ => vec2((t * 0.35).sin() * 1.2, 0.0),
    };
    let blinking = (t % 4.3) < 0.13;
    for side in [-1.0f32, 1.0] {
        let eye = rot(vec2(side * 21.0, 0.0));
        if blinking {
            painter.line_segment(
                [eye - vec2(6.0, 0.0) * s, eye + vec2(6.0, 0.0) * s],
                Stroke::new(2.2 * s, c.ink),
            );
        } else {
            painter.circle_filled(eye + look_at * s, 4.6 * s, c.ink);
            painter.circle_filled(
                eye + look_at * s + vec2(1.4, -1.4) * s,
                1.3 * s,
                Color32::WHITE,
            );
        }
        // Glasses: the teacher's trademark.
        if !appearance.minimal {
            painter.circle_stroke(eye, 14.0 * s, Stroke::new(2.2 * s, c.ink));
        }
        // Eyebrows rise a little when listening or thinking.
        let raise = match mood.phase {
            LessonPhase::Listening | LessonPhase::Thinking | LessonPhase::Transcribing => 3.0,
            _ => 0.0,
        };
        let brow: Vec<Pos2> = (0..=8)
            .map(|i| {
                let x = -9.0 + 18.0 * i as f32 / 8.0;
                rot(vec2(
                    side * 21.0 + x,
                    -22.0 - raise - (1.0 - (x / 9.0).powi(2)) * 3.0,
                ))
            })
            .collect();
        painter.add(Shape::line(brow, Stroke::new(2.6 * s, c.hair)));
        // Cheeks.
        painter.circle_filled(rot(vec2(side * 33.0, 20.0)), 8.0 * s, c.blush);
    }
    if !appearance.minimal {
        painter.line_segment(
            [rot(vec2(-7.0, -1.0)), rot(vec2(7.0, -1.0))],
            Stroke::new(2.0 * s, c.ink),
        );
    } else {
        // The minimal style trades accessories for a single quiet shirt detail.
        painter.line_segment(
            [
                pos2(center.x - 42.0 * s, shoulders_top + 56.0 * s),
                pos2(center.x + 42.0 * s, shoulders_top + 56.0 * s),
            ],
            Stroke::new(2.0 * s, c.shirt.gamma_multiply(0.45)),
        );
    }

    // Nose.
    painter.add(Shape::line(
        vec![
            rot(vec2(0.0, 8.0)),
            rot(vec2(-3.5, 18.0)),
            rot(vec2(1.5, 19.0)),
        ],
        Stroke::new(1.8 * s, c.skin_shade),
    ));

    // Mouth: a smile at rest, an open oval that follows the voice.
    let mouth = rot(vec2(0.0, 32.0));
    if open > 0.04 {
        let h = (3.0 + 15.0 * open.min(1.0)) * s;
        let w = (15.0 - 3.0 * open.min(1.0)) * s;
        let oval: Vec<Pos2> = (0..24)
            .map(|i| {
                let a = TAU * i as f32 / 24.0;
                mouth + vec2(a.cos() * w, a.sin() * h * 0.5 + h * 0.15)
            })
            .collect();
        painter.add(Shape::convex_polygon(oval, c.mouth, Stroke::NONE));
    } else {
        let smile: Vec<Pos2> = (0..=10)
            .map(|i| {
                let x = -12.0 + 24.0 * i as f32 / 10.0;
                rot(vec2(x, 32.0 + (1.0 - (x / 12.0).powi(2)) * 4.0))
            })
            .collect();
        painter.add(Shape::line(smile, Stroke::new(2.4 * s, c.mouth)));
    }

    // Thinking: a small bubble with dots, top right of the head.
    if matches!(
        mood.phase,
        LessonPhase::Thinking | LessonPhase::Transcribing
    ) {
        let bubble = head + vec2(78.0, -70.0) * s;
        painter.circle_filled(
            head + vec2(52.0, -42.0) * s,
            4.0 * s,
            accent.gamma_multiply(0.5),
        );
        painter.circle_filled(
            head + vec2(62.0, -54.0) * s,
            6.0 * s,
            accent.gamma_multiply(0.5),
        );
        painter.rect_filled(
            egui::Rect::from_center_size(bubble, vec2(54.0, 30.0) * s),
            15.0 * s,
            accent.gamma_multiply(0.5),
        );
        for i in 0..3 {
            let phase = (t * 3.0 - i as f32 * 0.6).sin() * 0.5 + 0.5;
            painter.circle_filled(
                bubble + vec2((i as f32 - 1.0) * 13.0, 0.0) * s,
                (3.0 + phase * 1.5) * s,
                Color32::WHITE.gamma_multiply(0.6 + phase * 0.4),
            );
        }
    }
}

/// Top half of an ellipse centered at `center`, as a polygon.
fn ellipse_top(center: Pos2, rx: f32, ry: f32) -> Vec<Pos2> {
    (0..=32)
        .map(|i| {
            let a = PI + PI * i as f32 / 32.0;
            center + vec2(a.cos() * rx, a.sin() * ry)
        })
        .collect()
}
