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

/// Draws the small Markdown subset the documents use: headings, lists,
/// paragraphs, bold text and tables.
pub fn show(ui: &mut egui::Ui, text: &str) {
    let mut paragraph = String::new();
    let mut table: Vec<Vec<String>> = Vec::new();
    let flush = |ui: &mut egui::Ui, paragraph: &mut String| {
        if !paragraph.is_empty() {
            ui.label(plain(paragraph));
            ui.add_space(6.0);
            paragraph.clear();
        }
    };
    for line in text.lines() {
        let line = line.trim_end();
        if line.starts_with('|') {
            flush(ui, &mut paragraph);
            let cells: Vec<String> = line
                .trim_matches('|')
                .split('|')
                .map(|c| plain(c.trim()))
                .collect();
            if !cells
                .iter()
                .all(|c| c.chars().all(|ch| ch == '-' || ch == ':'))
            {
                table.push(cells);
            }
            continue;
        }
        if !table.is_empty() {
            show_table(ui, &std::mem::take(&mut table));
        }
        if let Some(title) = line.strip_prefix("# ") {
            flush(ui, &mut paragraph);
            ui.label(
                RichText::new(title)
                    .family(theme::heading_family())
                    .size(20.0),
            );
            ui.add_space(4.0);
        } else if let Some(title) = line.strip_prefix("## ") {
            flush(ui, &mut paragraph);
            ui.add_space(4.0);
            ui.label(RichText::new(title).strong().size(15.0));
            ui.add_space(2.0);
        } else if let Some(item) = line.strip_prefix("- ") {
            flush(ui, &mut paragraph);
            crate::screens::setup::bullet(ui, &plain(item));
        } else if line.starts_with("  ") && !line.trim().is_empty() && paragraph.is_empty() {
            // Continuation of a list item: shown as its own indented line.
            ui.horizontal_wrapped(|ui| {
                ui.add_space(12.0);
                ui.label(plain(line.trim()));
            });
        } else if line.trim().is_empty() {
            flush(ui, &mut paragraph);
        } else {
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(line.trim());
        }
    }
    if !table.is_empty() {
        show_table(ui, &table);
    }
    flush(ui, &mut paragraph);
}

/// Each row as "Header: value" lines, which reads well at any width.
fn show_table(ui: &mut egui::Ui, rows: &[Vec<String>]) {
    let Some((header, rows)) = rows.split_first() else {
        return;
    };
    for row in rows {
        crate::widgets::card(ui, |ui| {
            for (title, value) in header.iter().zip(row) {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{title}:")).strong());
                    ui.label(value);
                });
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
