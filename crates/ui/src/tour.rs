//! Guided tour after the first-run setup: each step opens a screen, marks
//! its entry in the sidebar and says what it is for.

use egui::{Color32, RichText};
use kg_engine::Command;
use rust_i18n::t;

use crate::app::{KnowGiverApp, Screen};
use crate::theme;

/// Screens shown in the tour, in sidebar order.
const STEPS: [Screen; 11] = [
    Screen::Today,
    Screen::Edital,
    Screen::Plan,
    Screen::Reader,
    Screen::Board,
    Screen::Lesson,
    Screen::Quiz,
    Screen::Review,
    Screen::Discursive,
    Screen::Progress,
    Screen::Settings,
];

const CARD_WIDTH: f32 = 360.0;

/// The tour's screens; written answers only when the plan has a written test.
pub fn steps(writing: bool) -> Vec<Screen> {
    STEPS
        .into_iter()
        .filter(|s| writing || *s != Screen::Discursive)
        .collect()
}

fn text(screen: Screen) -> (String, String) {
    let (title, body) = match screen {
        Screen::Today => (t!("tour.today.title"), t!("tour.today.text")),
        Screen::Edital => (t!("tour.edital.title"), t!("tour.edital.text")),
        Screen::Plan => (t!("tour.plan.title"), t!("tour.plan.text")),
        Screen::Reader => (t!("tour.reader.title"), t!("tour.reader.text")),
        Screen::Board => (t!("tour.board.title"), t!("tour.board.text")),
        Screen::Lesson => (t!("tour.lesson.title"), t!("tour.lesson.text")),
        Screen::Quiz => (t!("tour.quiz.title"), t!("tour.quiz.text")),
        Screen::Review => (t!("tour.review.title"), t!("tour.review.text")),
        Screen::Discursive => (t!("tour.discursive.title"), t!("tour.discursive.text")),
        Screen::Progress => (t!("tour.progress.title"), t!("tour.progress.text")),
        Screen::Settings | Screen::Welcome | Screen::Setup => {
            (t!("tour.settings.title"), t!("tour.settings.text"))
        }
    };
    (title.into_owned(), body.into_owned())
}

/// Draws the tour card over the app and moves between screens.
pub fn show(app: &mut KnowGiverApp, ctx: &egui::Context, writing: bool) {
    let steps = steps(writing);
    let last = steps.len().saturating_sub(1);
    let index = app.ui.tour_step.min(last);
    let Some(&screen) = steps.get(index) else {
        return;
    };
    if app.ui.screen != screen {
        app.go_to(screen);
    }
    let p = theme::palette(ctx);
    let target = app
        .nav_rects
        .iter()
        .find(|(s, _)| *s == screen)
        .map(|(_, rect)| *rect);
    if let Some(rect) = target {
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("tour_mark"),
        ))
        .rect_stroke(
            rect.expand(3.0),
            6.0,
            egui::Stroke::new(2.5, p.highlight),
            egui::StrokeKind::Outside,
        );
    }
    let screen_rect = ctx.content_rect();
    let pos = target.map_or(egui::pos2(260.0, 40.0), |rect| {
        let y = (rect.top() - 12.0)
            .min(screen_rect.bottom() - 260.0)
            .max(12.0);
        egui::pos2(rect.right() + 18.0, y)
    });
    let (title, body) = text(screen);
    let mut go = None::<isize>;
    let mut finish = false;
    egui::Area::new(egui::Id::new("tour_card"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.surface)
                .stroke(egui::Stroke::new(1.5, p.highlight))
                .corner_radius(8.0)
                .shadow(egui::epaint::Shadow {
                    offset: [0, 6],
                    blur: 18,
                    spread: 0,
                    color: Color32::from_black_alpha(70),
                })
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(CARD_WIDTH);
                    ui.label(
                        RichText::new(t!("tour.step", n = index + 1, total = steps.len()))
                            .small()
                            .color(p.muted),
                    );
                    ui.label(
                        RichText::new(title)
                            .family(theme::strong_family())
                            .size(17.0),
                    );
                    ui.add_space(4.0);
                    ui.label(body);
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(t!("tour.skip")).clicked() {
                            finish = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let next = if index == last {
                                t!("tour.done")
                            } else {
                                t!("tour.next")
                            };
                            let button =
                                egui::Button::new(RichText::new(next).color(Color32::WHITE))
                                    .fill(p.accent);
                            if ui.add(button).clicked() {
                                if index == last {
                                    finish = true;
                                } else {
                                    go = Some(1);
                                }
                            }
                            if index > 0 && ui.button(t!("tour.back")).clicked() {
                                go = Some(-1);
                            }
                        });
                    });
                });
        });
    if finish {
        app.ui.tour_step = 0;
        app.dispatch(Command::FinishTour);
        let next = match &app.engine {
            Ok(engine) if engine.needs_onboarding() => Screen::Welcome,
            Ok(engine) if engine.active_plan().is_some() => Screen::Today,
            _ => Screen::Edital,
        };
        app.go_to(next);
    } else if let Some(step) = go {
        app.ui.tour_step = index.saturating_add_signed(step).min(last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_answers_only_with_a_written_test() {
        assert!(steps(true).contains(&Screen::Discursive));
        assert!(!steps(false).contains(&Screen::Discursive));
        assert_eq!(steps(false).first(), Some(&Screen::Today));
        assert_eq!(steps(false).last(), Some(&Screen::Settings));
    }
}
