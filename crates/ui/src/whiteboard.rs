//! Bounded plain-text formulas and small linear diagrams for the lesson board.

use egui::{Color32, FontId, Rect, Stroke, pos2, vec2};

/// Structured board content is presentation data; it is never evaluated as markup.
#[derive(Debug, PartialEq, Eq)]
pub enum Entry<'a> {
    Text(&'a str),
    Formula(&'a str),
    Diagram(Vec<&'a str>),
}

pub fn parse(text: &str) -> Entry<'_> {
    if let Some(expression) = text.strip_prefix("FORMULA:") {
        let expression = expression.trim();
        if !expression.is_empty() && expression.chars().count() <= 240 {
            return Entry::Formula(expression);
        }
    }
    if let Some(flow) = text.strip_prefix("DIAGRAMA:") {
        let nodes: Vec<_> = flow
            .split("->")
            .flat_map(|part| part.split('→'))
            .map(str::trim)
            .collect();
        if (2..=4).contains(&nodes.len())
            && nodes
                .iter()
                .all(|node| !node.is_empty() && node.chars().count() <= 80)
        {
            return Entry::Diagram(nodes);
        }
    }
    Entry::Text(text)
}

/// Draws a formula or diagram. Returns its required height, including breathing room.
pub fn paint(painter: &egui::Painter, area: Rect, entry: &Entry<'_>, ink: Color32) -> f32 {
    match entry {
        Entry::Formula(expression) => {
            let galley = painter.layout(
                (*expression).to_owned(),
                FontId::monospace(18.0),
                ink,
                (area.width() - 20.0).max(20.0),
            );
            let height = galley.size().y + 20.0;
            let rect = Rect::from_min_size(area.min, vec2(area.width(), height));
            painter.rect_filled(rect, 0.0, ink.gamma_multiply(0.06));
            painter.galley(area.min + vec2(10.0, 10.0), galley, ink);
            height + 12.0
        }
        Entry::Diagram(nodes) => {
            let horizontal = area.width() / nodes.len() as f32 >= 100.0;
            let gap = if horizontal { 20.0 } else { 19.0 };
            let width = if horizontal {
                (area.width() - gap * (nodes.len() - 1) as f32) / nodes.len() as f32
            } else {
                area.width()
            };
            let galleys: Vec<_> = nodes
                .iter()
                .map(|node| {
                    painter.layout(
                        (*node).to_owned(),
                        FontId::proportional(14.0),
                        ink,
                        (width - 16.0).max(20.0),
                    )
                })
                .collect();
            let height = galleys
                .iter()
                .map(|g| g.size().y + 18.0)
                .fold(34.0_f32, f32::max);
            for (i, galley) in galleys.into_iter().enumerate() {
                let offset = if horizontal {
                    vec2((width + gap) * i as f32, 0.0)
                } else {
                    vec2(0.0, (height + gap) * i as f32)
                };
                let node = Rect::from_min_size(area.min + offset, vec2(width, height));
                painter.rect_stroke(
                    node,
                    0.0,
                    Stroke::new(1.1, ink.gamma_multiply(0.75)),
                    egui::StrokeKind::Inside,
                );
                painter.galley(node.center() - galley.size() * 0.5, galley, ink);
                if i + 1 < nodes.len() {
                    let (start, end) = if horizontal {
                        (
                            node.right_center() + vec2(3.0, 0.0),
                            node.right_center() + vec2(gap - 3.0, 0.0),
                        )
                    } else {
                        (
                            node.center_bottom() + vec2(0.0, 3.0),
                            node.center_bottom() + vec2(0.0, gap - 3.0),
                        )
                    };
                    painter.arrow(start, end - start, Stroke::new(1.4, ink));
                }
            }
            if horizontal {
                height + 12.0
            } else {
                height * nodes.len() as f32 + gap * (nodes.len() - 1) as f32 + 12.0
            }
        }
        Entry::Text(text) => {
            let galley = painter.layout(
                format!("•  {text}"),
                FontId::proportional(16.0),
                ink,
                area.width(),
            );
            let height = galley.size().y;
            painter.galley(pos2(area.left(), area.top()), galley, ink);
            height + 12.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagrams_accept_unicode_and_reject_unbounded_or_empty_nodes() {
        assert_eq!(
            parse("DIAGRAMA: Edital -> Plano → Revisão"),
            Entry::Diagram(vec!["Edital", "Plano", "Revisão"])
        );
        for text in [
            "DIAGRAMA: a -> -> b",
            "DIAGRAMA: a -> b -> c -> d -> e",
            "FORMULA: ",
        ] {
            assert_eq!(parse(text), Entry::Text(text));
        }
        assert_eq!(
            parse("FORMULA: juros = capital × taxa × tempo"),
            Entry::Formula("juros = capital × taxa × tempo")
        );
    }
}
