//! "Working on it" panel for jobs that take minutes: the teacher thinking,
//! what is happening right now, progress, and a study tip that changes every
//! few seconds so the screen never looks frozen.

use egui::{RichText, vec2};
use kg_engine::{Engine, LessonPhase};
use rust_i18n::t;

use crate::{teacher, theme};

/// Seconds each tip stays on screen.
const TIP_SECS: f64 = 7.0;
/// Number of `working.tip_N` keys in the locale files (see `panel`).
const TIP_COUNT: usize = 6;

/// What the panel says.
pub struct Working<'a> {
    pub title: &'a str,
    /// What is happening right now.
    pub detail: &'a str,
    /// Expectation setting, such as "this can take a few minutes".
    pub note: &'a str,
    /// `0.0..=1.0`, or `None` when the length is unknown. Zero animates
    /// like `None`.
    pub progress: Option<f32>,
}

/// Draws the panel. Returns the area for buttons under the text.
pub fn panel(
    ui: &mut egui::Ui,
    engine: &Engine,
    working: &Working<'_>,
    add: impl FnOnce(&mut egui::Ui),
) {
    let p = theme::palette(ui.ctx());
    let time = ui.input(|i| i.time);
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(33));

    egui::Frame::new()
        .fill(p.surface)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(0.0)
        .inner_margin(egui::Margin::same(18))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(130.0, 150.0), egui::Sense::hover());
                teacher::paint(
                    ui,
                    rect,
                    teacher::Mood {
                        phase: LessonPhase::Thinking,
                        speech: 0.0,
                        mic: 0.0,
                    },
                    teacher::current(engine),
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("{}{}", working.title, dots(time)))
                            .family(theme::heading_family())
                            .size(20.0)
                            .color(p.text),
                    );
                    ui.add_space(2.0);
                    ui.label(RichText::new(working.detail).color(p.text));
                    ui.add_space(8.0);
                    // The track would vanish into the card without contrast.
                    ui.visuals_mut().extreme_bg_color = p.border;
                    // Until something finishes, a moving bar shows it is alive.
                    let bar = match working.progress {
                        Some(done) if done > 0.0 => egui::ProgressBar::new(done.min(1.0)),
                        _ => egui::ProgressBar::new(sweep(time)),
                    };
                    ui.add(
                        bar.desired_width(ui.available_width().min(420.0))
                            .desired_height(8.0),
                    );
                    ui.add_space(6.0);
                    ui.label(RichText::new(working.note).small().color(p.muted));
                    ui.add_space(10.0);
                    let tip = (time / TIP_SECS) as usize % TIP_COUNT;
                    ui.label(
                        RichText::new(t!("working.tip_label"))
                            .small()
                            .strong()
                            .color(p.accent),
                    );
                    let tip = match tip {
                        0 => t!("working.tip_1"),
                        1 => t!("working.tip_2"),
                        2 => t!("working.tip_3"),
                        3 => t!("working.tip_4"),
                        4 => t!("working.tip_5"),
                        _ => t!("working.tip_6"),
                    };
                    ui.label(RichText::new(tip).small().italics().color(p.muted));
                    ui.add_space(10.0);
                    add(ui);
                });
            });
        });
}

/// "", ".", "..", "..." in a loop.
fn dots(time: f64) -> &'static str {
    ["", ".", "..", "..."][(time * 2.0) as usize % 4]
}

/// A back-and-forth fill for progress of unknown length.
fn sweep(time: f64) -> f32 {
    let phase = (time * 0.6).fract() as f32;
    0.15 + 0.7 * (0.5 - 0.5 * (phase * std::f32::consts::TAU).cos())
}

/// "about 12 min" style estimate.
pub fn duration_text(duration: std::time::Duration) -> String {
    let minutes = duration.as_secs().div_ceil(60);
    if minutes <= 1 {
        t!("working.under_a_minute").into_owned()
    } else {
        t!("working.minutes", minutes = minutes).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_cycle_and_sweep_stays_inside_the_bar() {
        assert_eq!(dots(0.0), "");
        assert_eq!(dots(1.6), "...");
        for i in 0..100 {
            let value = sweep(f64::from(i) * 0.13);
            assert!((0.1..=0.9).contains(&value), "{value}");
        }
    }
}
