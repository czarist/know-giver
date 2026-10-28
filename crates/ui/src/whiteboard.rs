//! What the tutor writes on the lesson board: key points, formulas,
//! diagrams, tables, timelines, mind maps, exam tips, examples and pictures.
//!
//! Entries are presentation data parsed with fixed limits; nothing is
//! evaluated as markup.

use egui::{Color32, FontId, Rect, Stroke, pos2, vec2};
use rust_i18n::t;

use crate::theme;

#[derive(Debug, PartialEq, Eq)]
pub enum Entry<'a> {
    Text(&'a str),
    Formula(&'a str),
    Diagram(Vec<&'a str>),
    /// Header row first.
    Table(Vec<Vec<&'a str>>),
    /// `(when, what)` pairs.
    Timeline(Vec<(&'a str, &'a str)>),
    MindMap {
        center: &'a str,
        branches: Vec<&'a str>,
    },
    /// What the exam usually asks, or the classic trap.
    Tip(&'a str),
    Example(&'a str),
    /// A Wikipedia picture, by search term.
    Image(&'a str),
}

const CELL_CHARS: usize = 60;
const ITEM_CHARS: usize = 80;

fn short(text: &str, max: usize) -> bool {
    !text.is_empty() && text.chars().count() <= max
}

pub fn parse(text: &str) -> Entry<'_> {
    let field = |marker: &str| text.strip_prefix(marker).map(str::trim);
    if let Some(expression) = field("FORMULA:")
        && short(expression, 240)
    {
        return Entry::Formula(expression);
    }
    if let Some(flow) = field("DIAGRAMA:") {
        let nodes: Vec<_> = flow
            .split("->")
            .flat_map(|part| part.split('→'))
            .map(str::trim)
            .collect();
        if (2..=5).contains(&nodes.len()) && nodes.iter().all(|n| short(n, ITEM_CHARS)) {
            return Entry::Diagram(nodes);
        }
    }
    if let Some(table) = field("TABELA:") {
        let rows: Vec<Vec<&str>> = table
            .split(';')
            .map(|row| row.split('|').map(str::trim).collect())
            .collect();
        let columns = rows.first().map_or(0, Vec::len);
        if (2..=6).contains(&rows.len())
            && (2..=4).contains(&columns)
            && rows
                .iter()
                .all(|r| r.len() == columns && r.iter().all(|c| c.chars().count() <= CELL_CHARS))
        {
            return Entry::Table(rows);
        }
    }
    if let Some(events) = field("LINHA DO TEMPO:") {
        let items: Option<Vec<(&str, &str)>> = events
            .split('|')
            .map(|item| {
                let (when, what) = item.split_once(':')?;
                let (when, what) = (when.trim(), what.trim());
                (short(when, 20) && short(what, ITEM_CHARS)).then_some((when, what))
            })
            .collect();
        if let Some(items) = items
            && (2..=6).contains(&items.len())
        {
            return Entry::Timeline(items);
        }
    }
    if let Some(map) = field("MAPA:")
        && let Some((center, rest)) = map.split_once(':')
    {
        let branches: Vec<_> = rest.split('|').map(str::trim).collect();
        if short(center.trim(), ITEM_CHARS)
            && (2..=6).contains(&branches.len())
            && branches.iter().all(|b| short(b, ITEM_CHARS))
        {
            return Entry::MindMap {
                center: center.trim(),
                branches,
            };
        }
    }
    if let Some(tip) = field("DESTAQUE:")
        && short(tip, 320)
    {
        return Entry::Tip(tip);
    }
    if let Some(example) = field("EXEMPLO:")
        && short(example, 320)
    {
        return Entry::Example(example);
    }
    if let Some(query) = field("IMAGEM:")
        && short(query, 120)
    {
        return Entry::Image(query);
    }
    Entry::Text(text)
}

impl Entry<'_> {
    /// One line of plain text, for the transcript panel. Pictures have none.
    pub fn summary(&self) -> Option<String> {
        Some(match self {
            Self::Text(text) | Self::Formula(text) => (*text).to_owned(),
            Self::Diagram(nodes) => nodes.join(" → "),
            Self::Table(rows) => rows
                .iter()
                .map(|r| r.join(" | "))
                .collect::<Vec<_>>()
                .join(" ; "),
            Self::Timeline(items) => items
                .iter()
                .map(|(when, what)| format!("{when}: {what}"))
                .collect::<Vec<_>>()
                .join(" · "),
            Self::MindMap { center, branches } => format!("{center}: {}", branches.join(", ")),
            Self::Tip(text) => format!("{}: {text}", t!("lesson.board_tip")),
            Self::Example(text) => format!("{}: {text}", t!("lesson.board_example")),
            Self::Image(_) => return None,
        })
    }
}

/// A picture ready to draw: its loader URI and caption.
pub struct Picture<'a> {
    pub uri: &'a str,
    pub caption: &'a str,
}

/// Draws an entry at the top of `area`. Returns the height used, including
/// breathing room; 0 when there is nothing to draw (a picture still loading).
pub fn paint(
    ui: &egui::Ui,
    painter: &egui::Painter,
    area: Rect,
    entry: &Entry<'_>,
    ink: Color32,
    picture: Option<Picture<'_>>,
) -> f32 {
    let gap = 14.0;
    match entry {
        Entry::Text(text) => {
            let galley = painter.layout(
                format!("•  {text}"),
                FontId::new(16.0, theme::strong_family()),
                ink,
                area.width(),
            );
            let height = galley.size().y;
            painter.galley(area.min, galley, ink);
            height + gap
        }
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
            height + gap
        }
        Entry::Diagram(nodes) => diagram(painter, area, nodes, ink) + gap,
        Entry::Table(rows) => table(painter, area, rows, ink) + gap,
        Entry::Timeline(items) => timeline(painter, area, items, ink) + gap,
        Entry::MindMap { center, branches } => mind_map(painter, area, center, branches, ink) + gap,
        Entry::Tip(text) => {
            let accent = theme::palette(ui.ctx()).warning;
            labeled_box(
                painter,
                area,
                &t!("lesson.board_tip"),
                text,
                ink,
                accent,
                true,
            ) + gap
        }
        Entry::Example(text) => {
            labeled_box(
                painter,
                area,
                &t!("lesson.board_example"),
                text,
                ink,
                ink,
                false,
            ) + gap
        }
        Entry::Image(_) => match picture {
            Some(picture) => image(ui, painter, area, &picture, ink).map_or(0.0, |h| h + gap),
            None => 0.0,
        },
    }
}

fn diagram(painter: &egui::Painter, area: Rect, nodes: &[&str], ink: Color32) -> f32 {
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
        height
    } else {
        height * nodes.len() as f32 + gap * (nodes.len() - 1) as f32
    }
}

fn table(painter: &egui::Painter, area: Rect, rows: &[Vec<&str>], ink: Color32) -> f32 {
    let columns = rows.first().map_or(1, Vec::len).max(1);
    let width = area.width() / columns as f32;
    let pad = 7.0;
    let mut y = area.top();
    for (r, row) in rows.iter().enumerate() {
        let font = if r == 0 {
            FontId::new(14.0, theme::strong_family())
        } else {
            FontId::proportional(14.0)
        };
        let galleys: Vec<_> = row
            .iter()
            .map(|cell| {
                painter.layout(
                    (*cell).to_owned(),
                    font.clone(),
                    ink,
                    (width - pad * 2.0).max(20.0),
                )
            })
            .collect();
        let height = galleys.iter().map(|g| g.size().y).fold(0.0, f32::max) + pad * 2.0;
        if r == 0 {
            painter.rect_filled(
                Rect::from_min_size(pos2(area.left(), y), vec2(area.width(), height)),
                0.0,
                ink.gamma_multiply(0.10),
            );
        }
        for (c, galley) in galleys.into_iter().enumerate() {
            painter.galley(
                pos2(area.left() + width * c as f32 + pad, y + pad),
                galley,
                ink,
            );
        }
        y += height;
        // Chalk rule under each row; heavier under the header.
        painter.line_segment(
            [pos2(area.left(), y), pos2(area.right(), y)],
            Stroke::new(if r == 0 { 1.6 } else { 0.8 }, ink.gamma_multiply(0.6)),
        );
    }
    for c in 1..columns {
        let x = area.left() + width * c as f32;
        painter.line_segment(
            [pos2(x, area.top()), pos2(x, y)],
            Stroke::new(0.8, ink.gamma_multiply(0.45)),
        );
    }
    y - area.top()
}

fn timeline(painter: &egui::Painter, area: Rect, items: &[(&str, &str)], ink: Color32) -> f32 {
    let step = area.width() / items.len() as f32;
    if step < 90.0 {
        // Too narrow: one milestone per line, with the date in front.
        let mut y = area.top();
        for (when, what) in items {
            let galley = painter.layout(
                format!("{when}  —  {what}"),
                FontId::proportional(14.0),
                ink,
                area.width(),
            );
            let height = galley.size().y;
            painter.galley(pos2(area.left(), y), galley, ink);
            y += height + 6.0;
        }
        return y - area.top();
    }
    let line_y = area.top() + 26.0;
    painter.line_segment(
        [pos2(area.left(), line_y), pos2(area.right(), line_y)],
        Stroke::new(1.6, ink.gamma_multiply(0.8)),
    );
    let mut bottom = line_y;
    for (i, (when, what)) in items.iter().enumerate() {
        let x = area.left() + step * (i as f32 + 0.5);
        painter.circle_filled(pos2(x, line_y), 5.0, ink);
        painter.text(
            pos2(x, line_y - 10.0),
            egui::Align2::CENTER_BOTTOM,
            *when,
            FontId::new(14.0, theme::strong_family()),
            ink,
        );
        let galley = painter.layout(
            (*what).to_owned(),
            FontId::proportional(13.0),
            ink,
            (step - 10.0).max(30.0),
        );
        let size = galley.size();
        painter.galley(pos2(x - size.x / 2.0, line_y + 10.0), galley, ink);
        bottom = bottom.max(line_y + 10.0 + size.y);
    }
    bottom - area.top()
}

fn mind_map(
    painter: &egui::Painter,
    area: Rect,
    center: &str,
    branches: &[&str],
    ink: Color32,
) -> f32 {
    let heading = painter.layout(
        center.to_owned(),
        FontId::new(15.0, theme::strong_family()),
        ink,
        (area.width() * 0.6).max(40.0),
    );
    let hub = Rect::from_center_size(
        pos2(area.center().x, area.top() + heading.size().y / 2.0 + 10.0),
        heading.size() + vec2(24.0, 14.0),
    );
    painter.rect_filled(hub, 0.0, ink.gamma_multiply(0.12));
    painter.rect_stroke(hub, 0.0, Stroke::new(1.6, ink), egui::StrokeKind::Inside);
    painter.galley(hub.center() - heading.size() / 2.0, heading, ink);

    let per_row = if area.width() / branches.len() as f32 >= 110.0 {
        branches.len()
    } else {
        3.min(branches.len())
    };
    let gap = 10.0;
    let width = (area.width() - gap * (per_row - 1) as f32) / per_row as f32;
    let mut row_top = hub.bottom() + 26.0;
    let mut bottom = hub.bottom();
    for row in branches.chunks(per_row) {
        let galleys: Vec<_> = row
            .iter()
            .map(|b| {
                painter.layout(
                    (*b).to_owned(),
                    FontId::proportional(13.5),
                    ink,
                    (width - 14.0).max(20.0),
                )
            })
            .collect();
        let height = galleys.iter().map(|g| g.size().y).fold(0.0, f32::max) + 14.0;
        // Center a short last row.
        let used = width * row.len() as f32 + gap * (row.len() - 1) as f32;
        let left = area.left() + (area.width() - used) / 2.0;
        for (i, galley) in galleys.into_iter().enumerate() {
            let chip = Rect::from_min_size(
                pos2(left + (width + gap) * i as f32, row_top),
                vec2(width, height),
            );
            painter.line_segment(
                [hub.center_bottom(), chip.center_top()],
                Stroke::new(1.0, ink.gamma_multiply(0.6)),
            );
            painter.rect_stroke(
                chip,
                0.0,
                Stroke::new(1.0, ink.gamma_multiply(0.75)),
                egui::StrokeKind::Inside,
            );
            painter.galley(chip.center() - galley.size() / 2.0, galley, ink);
            bottom = bottom.max(chip.bottom());
        }
        row_top += height + gap;
    }
    bottom - area.top()
}

/// A text box with a small capital label above it.
fn labeled_box(
    painter: &egui::Painter,
    area: Rect,
    label: &str,
    text: &str,
    ink: Color32,
    accent: Color32,
    filled: bool,
) -> f32 {
    let pad = 10.0;
    let galley = painter.layout(
        text.to_owned(),
        FontId::proportional(15.0),
        ink,
        (area.width() - pad * 2.0 - 6.0).max(30.0),
    );
    let label_height = 18.0;
    let height = galley.size().y + pad * 2.0 + label_height;
    let rect = Rect::from_min_size(area.min, vec2(area.width(), height));
    if filled {
        painter.rect_filled(rect, 0.0, accent.gamma_multiply(0.16));
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.6, accent),
            egui::StrokeKind::Inside,
        );
    } else {
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(4.0, rect.height())),
            0.0,
            accent.gamma_multiply(0.7),
        );
    }
    painter.text(
        rect.min + vec2(pad + 4.0, pad - 2.0),
        egui::Align2::LEFT_TOP,
        label.to_uppercase(),
        FontId::new(11.5, theme::strong_family()),
        if filled {
            accent
        } else {
            ink.gamma_multiply(0.7)
        },
    );
    painter.galley(
        rect.min + vec2(pad + 4.0, pad + label_height - 2.0),
        galley,
        ink,
    );
    height
}

/// A picture pinned to the board with a paper border and its source.
/// `None` while the picture is still being decoded.
fn image(
    ui: &egui::Ui,
    painter: &egui::Painter,
    area: Rect,
    picture: &Picture<'_>,
    ink: Color32,
) -> Option<f32> {
    let max_height = (area.height() - 26.0).clamp(60.0, 170.0);
    let poll = ui
        .ctx()
        .try_load_texture(
            picture.uri,
            egui::TextureOptions::LINEAR,
            egui::SizeHint::Height((max_height * ui.ctx().pixels_per_point()) as u32),
        )
        .ok()?;
    let egui::load::TexturePoll::Ready { texture } = poll else {
        ui.ctx().request_repaint();
        return None;
    };
    let aspect = texture.size.x / texture.size.y.max(1.0);
    let mut size = vec2(max_height * aspect, max_height);
    if size.x > area.width() - 12.0 {
        size *= (area.width() - 12.0) / size.x;
    }
    let border = 5.0;
    let frame = Rect::from_min_size(area.min, size + egui::Vec2::splat(border * 2.0));
    painter.rect_filled(
        frame.translate(vec2(4.0, 4.0)),
        0.0,
        Color32::from_black_alpha(90),
    );
    painter.rect_filled(frame, 0.0, Color32::from_rgb(0xF6, 0xF3, 0xEC));
    painter.image(
        texture.id,
        Rect::from_min_size(frame.min + vec2(border, border), size),
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    let caption = painter.layout(
        t!("lesson.board_image_source", title = picture.caption).into_owned(),
        FontId::proportional(11.5),
        ink.gamma_multiply(0.65),
        area.width(),
    );
    let caption_height = caption.size().y;
    painter.galley(pos2(area.left(), frame.bottom() + 5.0), caption, ink);
    Some(frame.height() + 5.0 + caption_height)
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
            "DIAGRAMA: a -> b -> c -> d -> e -> f",
            "FORMULA: ",
        ] {
            assert_eq!(parse(text), Entry::Text(text));
        }
        assert_eq!(
            parse("FORMULA: juros = capital × taxa × tempo"),
            Entry::Formula("juros = capital × taxa × tempo")
        );
    }

    #[test]
    fn rich_entries_parse_within_limits() {
        assert_eq!(
            parse("TABELA: Lei | Prazo ; PPA | 4 anos ; LOA | 1 ano"),
            Entry::Table(vec![
                vec!["Lei", "Prazo"],
                vec!["PPA", "4 anos"],
                vec!["LOA", "1 ano"]
            ])
        );
        // Ragged rows are not a table.
        let ragged = "TABELA: a | b ; c";
        assert_eq!(parse(ragged), Entry::Text(ragged));
        assert_eq!(
            parse("LINHA DO TEMPO: 1988: Constituição | 2000: LRF"),
            Entry::Timeline(vec![("1988", "Constituição"), ("2000", "LRF")])
        );
        assert_eq!(
            parse("MAPA: Orçamento: PPA | LDO | LOA"),
            Entry::MindMap {
                center: "Orçamento",
                branches: vec!["PPA", "LDO", "LOA"]
            }
        );
        assert_eq!(parse("DESTAQUE: Cai muito"), Entry::Tip("Cai muito"));
        assert_eq!(
            parse("EXEMPLO: Uma prefeitura"),
            Entry::Example("Uma prefeitura")
        );
        assert_eq!(
            parse("IMAGEM: Palácio do Planalto"),
            Entry::Image("Palácio do Planalto")
        );
        assert_eq!(parse("IMAGEM: Brasília").summary(), None);
    }
}
