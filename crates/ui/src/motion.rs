//! Small motion for moments that matter: a verdict that slides in, a pulse on
//! the right answer, a shake on a wrong one and confetti when something is
//! finished. Short, and off when the student turns animations off.

use egui::{Color32, Context, Id, LayerId, Order, Pos2, Rect, Shape, Stroke, Vec2};

use crate::theme;

/// How long confetti stays on screen.
const CONFETTI_LIFE: f64 = 1.9;

fn enabled_id() -> Id {
    Id::new("kg_motion_enabled")
}

fn confetti_id() -> Id {
    Id::new("kg_confetti")
}

/// Follows the student's setting; called once per frame by the app.
pub fn set_enabled(ctx: &Context, enabled: bool) {
    ctx.data_mut(|d| d.insert_temp(enabled_id(), enabled));
}

/// Motion is on. Screenshots in tests are always still.
pub fn enabled(ctx: &Context) -> bool {
    !cfg!(test)
        && ctx
            .data(|d| d.get_temp::<bool>(enabled_id()))
            .unwrap_or(true)
}

/// True only the first frame `id` is asked about, e.g. the frame a result
/// appears, so its sound and celebration run once.
pub fn first_time(ctx: &Context, id: Id) -> bool {
    let id = id.with("kg_seen");
    ctx.data_mut(|d| {
        if d.get_temp::<()>(id).is_some() {
            false
        } else {
            d.insert_temp(id, ());
            true
        }
    })
}

/// Progress `0.0..=1.0` of an animation that starts the first frame `id` is
/// drawn and lasts `duration` seconds; 1.0 at once when motion is off.
pub fn progress(ctx: &Context, id: Id, duration: f64) -> f32 {
    if !enabled(ctx) {
        return 1.0;
    }
    let now = ctx.input(|i| i.time);
    let start = ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(id.with("kg_start"), || now));
    let t = ((now - start) / duration).clamp(0.0, 1.0) as f32;
    if t < 1.0 {
        ctx.request_repaint();
    }
    t
}

/// Eases out: fast at first, settling at the end.
pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Shows `add` fading and sliding up into place the first time `id` is drawn.
pub fn appear<R>(ui: &mut egui::Ui, id: Id, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let t = ease_out(progress(ui.ctx(), id, 0.28));
    ui.add_space((1.0 - t) * 10.0);
    let inner = ui.scope(|ui| {
        ui.multiply_opacity(t);
        add(ui)
    });
    inner.inner
}

/// Shows `add` with a short sideways shake the first time `id` is drawn.
pub fn shake<R>(ui: &mut egui::Ui, id: Id, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let t = progress(ui.ctx(), id, 0.42);
    let offset = (t * 28.0).sin().abs() * 7.0 * (1.0 - t);
    ui.horizontal(|ui| {
        ui.add_space(offset);
        ui.vertical(|ui| add(ui)).inner
    })
    .inner
}

/// A ring that grows from `rect` and fades, drawn over it.
pub fn pulse(ui: &egui::Ui, id: Id, rect: Rect, color: Color32) {
    let t = progress(ui.ctx(), id, 0.55);
    if t >= 1.0 {
        return;
    }
    let grow = ease_out(t) * 9.0;
    let painter = ui
        .ctx()
        .layer_painter(LayerId::new(Order::Foreground, id.with("pulse")));
    painter.rect_stroke(
        rect.expand(grow),
        2.0,
        Stroke::new(2.5 * (1.0 - t), color.gamma_multiply(1.0 - t)),
        egui::StrokeKind::Outside,
    );
}

#[derive(Debug, Clone, Copy)]
struct Piece {
    pos: Pos2,
    vel: Vec2,
    color: Color32,
    size: Vec2,
    angle: f32,
    spin: f32,
}

#[derive(Debug, Clone, Default)]
struct Confetti {
    pieces: Vec<Piece>,
    /// When the last burst was thrown.
    last: f64,
}

/// Throws `count` pieces of confetti up from `origin`.
pub fn confetti(ctx: &Context, origin: Pos2, count: usize) {
    if !enabled(ctx) {
        return;
    }
    let p = theme::palette(ctx);
    let colors = [
        p.accent,
        p.success,
        p.warning,
        p.highlight,
        Color32::from_rgb(90, 140, 230),
    ];
    let now = ctx.input(|i| i.time);
    let mut seed = (now * 1_000.0) as u64 ^ 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        // xorshift: enough for confetti, no dependency.
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 10_000) as f32 / 10_000.0
    };
    let mut pieces = Vec::with_capacity(count);
    for i in 0..count {
        // Mostly upwards, spread over about 140 degrees.
        let angle = -std::f32::consts::FRAC_PI_2 + (next() - 0.5) * 2.4;
        let speed = 320.0 + next() * 420.0;
        pieces.push(Piece {
            pos: origin + Vec2::new((next() - 0.5) * 30.0, 0.0),
            vel: Vec2::angled(angle) * speed,
            color: colors[i % colors.len()],
            size: Vec2::new(5.0 + next() * 5.0, 3.0 + next() * 3.0),
            angle: next() * std::f32::consts::TAU,
            spin: (next() - 0.5) * 14.0,
        });
    }
    ctx.data_mut(|d| {
        let state = d.get_temp_mut_or_default::<Confetti>(confetti_id());
        state.last = now;
        state.pieces.extend(pieces);
    });
    ctx.request_repaint();
}

/// Confetti from the upper middle of the window, for a whole-screen moment.
pub fn celebrate(ctx: &Context, count: usize) {
    let rect = ctx.content_rect();
    confetti(
        ctx,
        Pos2::new(rect.center().x, rect.top() + rect.height() * 0.42),
        count,
    );
}

/// Moves and draws the confetti; called once per frame by the app.
pub fn paint(ctx: &Context) {
    let Some(mut state) = ctx.data(|d| d.get_temp::<Confetti>(confetti_id())) else {
        return;
    };
    if state.pieces.is_empty() {
        return;
    }
    let now = ctx.input(|i| i.time);
    let age = now - state.last;
    if age > CONFETTI_LIFE || !enabled(ctx) {
        ctx.data_mut(|d| d.remove_temp::<Confetti>(confetti_id()));
        return;
    }
    let dt = ctx.input(|i| i.stable_dt).min(0.05);
    let fade = (((CONFETTI_LIFE - age) / 0.5).min(1.0)) as f32;
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, confetti_id()));
    let bottom = ctx.content_rect().bottom() + 20.0;
    state.pieces.retain_mut(|piece| {
        piece.vel.y += 900.0 * dt;
        piece.vel *= 1.0 - 1.6 * dt;
        piece.pos += piece.vel * dt;
        piece.angle += piece.spin * dt;
        if piece.pos.y > bottom {
            return false;
        }
        // Flutter: the width shrinks and grows as the piece turns.
        let half = Vec2::new(
            piece.size.x * piece.angle.cos().abs().max(0.2),
            piece.size.y,
        ) * 0.5;
        let rot = Vec2::angled(piece.angle * 0.5);
        let corner =
            |x: f32, y: f32| piece.pos + Vec2::new(x * rot.x - y * rot.y, x * rot.y + y * rot.x);
        painter.add(Shape::convex_polygon(
            vec![
                corner(-half.x, -half.y),
                corner(half.x, -half.y),
                corner(half.x, half.y),
                corner(-half.x, half.y),
            ],
            piece.color.gamma_multiply(fade),
            Stroke::NONE,
        ));
        true
    });
    ctx.data_mut(|d| d.insert_temp(confetti_id(), state));
    ctx.request_repaint();
}
