//! Terms of use and privacy policy, shown in the app's language. The texts
//! live in `docs/legal` and are compiled in, so the app shows exactly the
//! version the student agrees to.

use egui::RichText;

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalDoc {
    Terms,
    Privacy,
}

impl LegalDoc {
    pub const ALL: [Self; 2] = [Self::Terms, Self::Privacy];

    /// The document in `locale`: Portuguese, or English for any other.
    pub fn text(self, locale: &str) -> &'static str {
        let portuguese = locale.starts_with("pt");
        match (self, portuguese) {
            (Self::Terms, true) => include_str!("../../../docs/legal/terms-of-use.pt-BR.md"),
            (Self::Terms, false) => include_str!("../../../docs/legal/terms-of-use.en.md"),
            (Self::Privacy, true) => include_str!("../../../docs/legal/privacy-policy.pt-BR.md"),
            (Self::Privacy, false) => include_str!("../../../docs/legal/privacy-policy.en.md"),
        }
    }
}

/// A piece of a document, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Title(String),
    Heading(String),
    /// Lines of one paragraph, joined.
    Paragraph(String),
    /// Lines of one list item, joined.
    Item(String),
    /// Header row first.
    Table(Vec<Vec<String>>),
}

/// Reads the small Markdown subset the documents use. Lines of a paragraph
/// or list item are joined, so the text wraps to the width on screen.
fn parse(text: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut open: Option<Part> = None;
    let mut table: Vec<Vec<String>> = Vec::new();
    let close = |parts: &mut Vec<Part>, open: &mut Option<Part>, table: &mut Vec<Vec<String>>| {
        if let Some(part) = open.take() {
            parts.push(part);
        }
        if !table.is_empty() {
            parts.push(Part::Table(std::mem::take(table)));
        }
    };
    for line in text.lines() {
        let line = line.trim_end();
        let trimmed = line.trim_start();
        if trimmed.starts_with('|') {
            if let Some(part) = open.take() {
                parts.push(part);
            }
            let cells: Vec<String> = trimmed
                .trim_matches('|')
                .split('|')
                .map(|c| plain(c.trim()))
                .collect();
            let separator = cells
                .iter()
                .all(|c| c.chars().all(|ch| ch == '-' || ch == ':'));
            if !separator {
                table.push(cells);
            }
        } else if let Some(title) = line.strip_prefix("# ") {
            close(&mut parts, &mut open, &mut table);
            parts.push(Part::Title(plain(title)));
        } else if let Some(title) = line.strip_prefix("## ") {
            close(&mut parts, &mut open, &mut table);
            parts.push(Part::Heading(plain(title)));
        } else if let Some(item) = trimmed.strip_prefix("- ") {
            close(&mut parts, &mut open, &mut table);
            open = Some(Part::Item(plain(item)));
        } else if trimmed.is_empty() {
            close(&mut parts, &mut open, &mut table);
        } else {
            if !table.is_empty() {
                parts.push(Part::Table(std::mem::take(&mut table)));
            }
            match &mut open {
                Some(Part::Paragraph(text) | Part::Item(text)) => {
                    text.push(' ');
                    text.push_str(&plain(trimmed));
                }
                _ => open = Some(Part::Paragraph(plain(trimmed))),
            }
        }
    }
    close(&mut parts, &mut open, &mut table);
    parts
}

/// Draws a document: headings, paragraphs, lists and tables.
pub fn show(ui: &mut egui::Ui, text: &str) {
    for part in parse(text) {
        match part {
            Part::Title(title) => {
                ui.label(
                    RichText::new(title)
                        .family(theme::heading_family())
                        .size(20.0),
                );
                ui.add_space(4.0);
            }
            Part::Heading(title) => {
                ui.add_space(6.0);
                ui.label(RichText::new(title).strong().size(15.0));
                ui.add_space(2.0);
            }
            Part::Paragraph(text) => {
                wrapped(ui, &text);
                ui.add_space(6.0);
            }
            Part::Item(text) => {
                crate::screens::setup::bullet(ui, &text);
                ui.add_space(2.0);
            }
            Part::Table(rows) => show_table(ui, &rows),
        }
    }
}

/// Text that wraps at the width available here.
fn wrapped(ui: &mut egui::Ui, text: &str) {
    let width = ui.available_width();
    ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
        ui.set_max_width(width);
        ui.add(egui::Label::new(text).wrap());
    });
}

/// Each row as "Header: value" lines, which reads well at any width.
fn show_table(ui: &mut egui::Ui, rows: &[Vec<String>]) {
    let Some((header, rows)) = rows.split_first() else {
        return;
    };
    for row in rows {
        crate::widgets::card(ui, |ui| {
            for (title, value) in header.iter().zip(row) {
                wrapped(ui, &format!("{title}: {value}"));
            }
        });
        ui.add_space(4.0);
    }
}

/// Text without Markdown emphasis and code marks.
fn plain(text: &str) -> String {
    text.replace("**", "").replace('`', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_document_exists_in_both_languages_with_the_terms_date() {
        for doc in LegalDoc::ALL {
            for locale in crate::i18n::LOCALES {
                let text = doc.text(locale);
                assert!(text.starts_with("# "), "{doc:?} {locale}");
                assert!(text.contains("2026"), "{doc:?} {locale} has no date");
                assert!(text.contains("trentin.software"), "{doc:?} {locale}");
            }
        }
        // The date in the documents is the version the app records.
        let date = kg_engine::TERMS_VERSION;
        assert_eq!(date, "2026-10-29");
        assert!(
            LegalDoc::Terms
                .text("pt-BR")
                .contains("29 de outubro de 2026")
        );
        assert!(LegalDoc::Terms.text("en").contains("October 29, 2026"));
    }

    #[test]
    fn lines_of_a_paragraph_or_item_are_joined() {
        let parts = parse(
            "# Title

## 1. Who

One line
and the next.

- an item
  that wraps
- **bold** item

| A | B |
|---|---|
| x | y |
",
        );
        assert_eq!(
            parts,
            [
                Part::Title("Title".into()),
                Part::Heading("1. Who".into()),
                Part::Paragraph("One line and the next.".into()),
                Part::Item("an item that wraps".into()),
                Part::Item("bold item".into()),
                Part::Table(vec![
                    vec!["A".into(), "B".into()],
                    vec!["x".into(), "y".into()],
                ]),
            ]
        );
    }

    #[test]
    fn no_document_line_is_lost() {
        // Every word of every document reaches some part.
        for doc in LegalDoc::ALL {
            for locale in crate::i18n::LOCALES {
                let text = doc.text(locale);
                let parts = parse(text);
                let shown: String = parts
                    .iter()
                    .map(|p| match p {
                        Part::Title(t) | Part::Heading(t) | Part::Paragraph(t) | Part::Item(t) => {
                            t.clone()
                        }
                        Part::Table(rows) => rows.concat().join(" "),
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                for word in plain(text).split_whitespace() {
                    let word = word.trim_matches(|c: char| "#-|:".contains(c));
                    if !word.is_empty() {
                        assert!(shown.contains(word), "{doc:?} {locale}: {word}");
                    }
                }
            }
        }
    }

    #[test]
    fn terms_say_the_ai_can_be_wrong() {
        assert!(LegalDoc::Terms.text("pt-BR").contains("podem conter erros"));
        assert!(
            LegalDoc::Terms
                .text("pt-BR")
                .contains("limites da IA que você escolheu")
        );
        assert!(LegalDoc::Terms.text("en").contains("may be wrong"));
        assert!(
            LegalDoc::Terms
                .text("en")
                .contains("limits of the AI you chose")
        );
    }
}
