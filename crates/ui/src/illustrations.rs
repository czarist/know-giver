//! Original vector study illustrations for empty states, using the current palette.

use egui::{Pos2, Rect, Shape, Stroke, vec2};

use crate::{icons::Icon, theme};

/// A quiet visual cue for the empty surface; fixed copy remains in the calling view.
pub fn empty(ui: &mut egui::Ui, kind: Icon) {
    let (rect, _) = ui.allocate_exact_size(vec2(240.0, 142.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let p = theme::palette(ui.ctx());
    let origin = rect.min;
    let at = |x: f32, y: f32| origin + vec2(x, y);
    painter.circle_filled(at(120.0, 69.0), 63.0, p.accent.gamma_multiply(0.07));
    painter.line_segment(
        [at(30.0, 130.0), at(210.0, 130.0)],
        Stroke::new(1.4, p.border),
    );
    match kind {
        Icon::Progress => {
            for (x, height) in [(67.0, 29.0), (96.0, 52.0), (125.0, 76.0)] {
                painter.rect_filled(
                    Rect::from_min_max(at(x, 128.0 - height), at(x + 18.0, 128.0)),
                    0.0,
                    p.accent.gamma_multiply(0.65),
                );
            }
            painter.add(Shape::line(
                vec![
                    at(163.0, 126.0),
                    at(162.0, 94.0),
                    at(174.0, 74.0),
                    at(173.0, 43.0),
                ],
                Stroke::new(2.4, p.accent),
            ));
            leaf(&painter, at(162.0, 97.0), at(141.0, 72.0), p.accent);
            leaf(&painter, at(174.0, 76.0), at(195.0, 52.0), p.highlight);
            leaf(&painter, at(173.0, 53.0), at(158.0, 32.0), p.accent);
        }
        Icon::Quiz | Icon::Review => {
            for (x, y) in [(62.0, 34.0), (75.0, 25.0), (88.0, 16.0)] {
                let card = Rect::from_min_size(at(x, y), vec2(100.0, 110.0));
                painter.rect_filled(card, 0.0, p.surface);
                painter.rect_stroke(
                    card,
                    0.0,
                    Stroke::new(1.4, p.border),
                    egui::StrokeKind::Inside,
                );
            }
            for (i, width) in [62.0, 44.0, 53.0].into_iter().enumerate() {
                let y = 43.0 + 27.0 * i as f32;
                painter.circle_stroke(
                    at(105.0, y),
                    5.0,
                    Stroke::new(1.5, if i == 1 { p.accent } else { p.border }),
                );
                painter.line_segment(
                    [at(118.0, y), at(118.0 + width - 18.0, y)],
                    Stroke::new(2.5, p.muted.gamma_multiply(0.5)),
                );
            }
            painter.add(Shape::line(
                vec![at(101.0, 70.0), at(104.0, 73.0), at(110.0, 66.0)],
                Stroke::new(2.0, p.accent),
            ));
        }
        Icon::Plan => {
            let paper = Rect::from_min_size(at(55.0, 20.0), vec2(131.0, 106.0));
            painter.rect_filled(paper, 0.0, p.surface);
            painter.rect_stroke(
                paper,
                0.0,
                Stroke::new(1.3, p.border),
                egui::StrokeKind::Inside,
            );
            let stops = [
                at(77.0, 103.0),
                at(120.0, 84.0),
                at(97.0, 55.0),
                at(159.0, 39.0),
            ];
            painter.add(Shape::line(stops.to_vec(), Stroke::new(2.0, p.accent)));
            for stop in stops {
                painter.circle_filled(stop, 5.5, p.surface);
                painter.circle_stroke(stop, 5.5, Stroke::new(2.0, p.accent));
            }
            painter.line_segment(
                [at(159.0, 39.0), at(159.0, 16.0)],
                Stroke::new(2.0, p.highlight),
            );
            painter.add(Shape::convex_polygon(
                vec![at(160.0, 16.0), at(178.0, 21.0), at(160.0, 27.0)],
                p.highlight,
                Stroke::NONE,
            ));
        }
        _ => {
            let left = vec![
                at(120.0, 41.0),
                at(60.0, 29.0),
                at(51.0, 111.0),
                at(120.0, 124.0),
            ];
            let right = vec![
                at(120.0, 41.0),
                at(180.0, 29.0),
                at(189.0, 111.0),
                at(120.0, 124.0),
            ];
            for page in [left, right] {
                painter.add(Shape::convex_polygon(
                    page,
                    p.surface,
                    Stroke::new(1.6, p.accent),
                ));
            }
            painter.line_segment(
                [at(120.0, 42.0), at(120.0, 123.0)],
                Stroke::new(1.4, p.accent),
            );
            for y in [55.0, 68.0, 81.0, 94.0] {
                painter.line_segment(
                    [at(70.0, y), at(106.0, y + 6.0)],
                    Stroke::new(2.0, p.border),
                );
                painter.line_segment(
                    [at(135.0, y + 6.0), at(170.0, y)],
                    Stroke::new(2.0, p.border),
                );
            }
            painter.add(Shape::convex_polygon(
                vec![
                    at(150.0, 36.0),
                    at(162.0, 33.0),
                    at(163.0, 76.0),
                    at(156.0, 71.0),
                    at(150.0, 78.0),
                ],
                p.highlight,
                Stroke::NONE,
            ));
        }
    }
}

fn leaf(painter: &egui::Painter, start: Pos2, tip: Pos2, color: egui::Color32) {
    let middle = start.lerp(tip, 0.5);
    let normal = vec2(-(tip.y - start.y), tip.x - start.x).normalized() * 7.0;
    painter.add(Shape::convex_polygon(
        vec![start, middle + normal, tip, middle - normal],
        color,
        Stroke::NONE,
    ));
}
