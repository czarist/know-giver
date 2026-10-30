//! Terms of use, privacy policy and partner access terms. The texts live in
//! `docs/legal` and are compiled in, so the app shows exactly the version
//! the student agrees to.
//!
//! The law that applies is the region's, not the language's. Each language
//! has a version for the regions where it is used; the English version covers
//! every public region and is shown when the language has no version for the
//! chosen region. Outside the public regions only the partner access terms
//! apply.

use egui::RichText;
use kg_engine::Region;

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalDoc {
    Terms,
    Privacy,
    /// Use limited to partners authorized in writing.
    Partner,
}

macro_rules! doc {
    ($name:literal) => {
        include_str!(concat!("../../../docs/legal/", $name, ".md"))
    };
}

impl LegalDoc {
    /// The documents that apply in `region`.
    pub fn for_region(region: Region) -> &'static [Self] {
        if region.is_public() {
            &[Self::Terms, Self::Privacy]
        } else {
            &[Self::Partner]
        }
    }

    /// The language a document is shown in: the UI language when it has a
    /// version for the region, otherwise English.
    pub fn language(self, locale: &str, region: Region) -> &'static str {
        if self == Self::Partner {
            return match locale {
                "pt-BR" => "pt-BR",
                "ja" => "ja",
                "zh-Hans" => "zh-Hans",
                "zh-Hant" => "zh-Hant",
                _ => "en",
            };
        }
        match (locale, region) {
            ("pt-BR", Region::Br) => "pt-BR",
            ("ja", Region::Jp) => "ja",
            ("zh-Hant", Region::Tw | Region::Hk) => "zh-Hant",
            ("zh-Hans", Region::Sg) => "zh-Hans",
            _ => "en",
        }
    }

    pub fn text(self, locale: &str, region: Region) -> &'static str {
        match (self, self.language(locale, region)) {
            (Self::Terms, "pt-BR") => doc!("terms-of-use.pt-BR"),
            (Self::Terms, "ja") => doc!("terms-of-use.ja"),
            (Self::Terms, "zh-Hant") => doc!("terms-of-use.zh-Hant"),
            (Self::Terms, "zh-Hans") => doc!("terms-of-use.zh-Hans"),
            (Self::Terms, _) => doc!("terms-of-use.en"),
            (Self::Privacy, "pt-BR") => doc!("privacy-policy.pt-BR"),
            (Self::Privacy, "ja") => doc!("privacy-policy.ja"),
            (Self::Privacy, "zh-Hant") => doc!("privacy-policy.zh-Hant"),
            (Self::Privacy, "zh-Hans") => doc!("privacy-policy.zh-Hans"),
            (Self::Privacy, _) => doc!("privacy-policy.en"),
            (Self::Partner, "pt-BR") => doc!("partner-access.pt-BR"),
            (Self::Partner, "ja") => doc!("partner-access.ja"),
            (Self::Partner, "zh-Hant") => doc!("partner-access.zh-Hant"),
            (Self::Partner, "zh-Hans") => doc!("partner-access.zh-Hans"),
            (Self::Partner, _) => doc!("partner-access.en"),
        }
    }
}

/// Chinese and Japanese characters: lines join without a space.
fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
}

/// Joins a continuation line to a paragraph: with a space between words,
/// without one between Chinese or Japanese characters.
fn join(text: &mut String, next: &str) {
    let cjk = text.chars().last().is_some_and(is_cjk) || next.chars().next().is_some_and(is_cjk);
    if !cjk {
        text.push(' ');
    }
    text.push_str(next);
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
                Some(Part::Paragraph(text) | Part::Item(text)) => join(text, &plain(trimmed)),
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

    /// Every document a student can be shown, with its region.
    fn all_shown() -> Vec<(LegalDoc, &'static str, Region, &'static str)> {
        let mut docs = Vec::new();
        for region in Region::ALL {
            for doc in LegalDoc::for_region(region) {
                for locale in crate::i18n::LOCALES {
                    docs.push((*doc, locale, region, doc.text(locale, region)));
                }
            }
        }
        docs
    }

    #[test]
    fn every_document_is_complete_and_dated_with_the_terms_version() {
        for (doc, locale, region, text) in all_shown() {
            let at = format!("{doc:?} {locale} {region:?}");
            assert!(text.starts_with("# "), "{at}");
            assert!(
                text.contains(kg_engine::TERMS_VERSION),
                "{at} has no version date"
            );
            assert!(text.contains("trentin.software"), "{at}");
        }
    }

    #[test]
    fn law_follows_the_region() {
        // A Japanese UI in Japan reads the Japanese version; in Brazil, the
        // English one, which covers every public region.
        assert_eq!(LegalDoc::Terms.language("ja", Region::Jp), "ja");
        assert_eq!(LegalDoc::Terms.language("ja", Region::Br), "en");
        assert_eq!(LegalDoc::Terms.language("zh-Hant", Region::Hk), "zh-Hant");
        assert_eq!(LegalDoc::Terms.language("zh-Hans", Region::Sg), "zh-Hans");
        assert_eq!(LegalDoc::Terms.language("pt-BR", Region::Eu), "en");
        assert_eq!(LegalDoc::for_region(Region::Other), [LegalDoc::Partner]);
        let en = [
            LegalDoc::Terms.text("en", Region::Us),
            LegalDoc::Privacy.text("en", Region::Us),
        ]
        .concat();
        for law in ["LGPD", "APPI", "Taiwan", "Hong Kong", "Singapore", "GDPR"] {
            assert!(en.contains(law), "{law}");
        }
    }

    #[test]
    fn partner_terms_name_no_country() {
        for locale in crate::i18n::LOCALES {
            let text = LegalDoc::Partner.text(locale, Region::Other);
            for name in [
                "China", "中国", "中國", "中华", "中華", "大陆", "大陸", "PRC",
            ] {
                assert!(!text.contains(name), "{locale} mentions {name}");
            }
        }
    }

    #[test]
    fn every_terms_text_says_the_ai_can_be_wrong() {
        let says = |text: &str, phrases: &[&str]| phrases.iter().any(|p| text.contains(p));
        for (doc, locale, region, text) in all_shown() {
            if doc == LegalDoc::Privacy {
                continue;
            }
            let at = format!("{doc:?} {locale} {region:?}");
            assert!(
                says(
                    text,
                    &["podem conter erros", "may be wrong", "誤り", "错误", "錯誤"]
                ),
                "{at}: AI errors"
            );
            assert!(
                says(text, &["limites da IA", "limits of the AI", "限界", "限制"]),
                "{at}: limits of the chosen AI"
            );
        }
    }

    #[test]
    fn chinese_and_japanese_lines_join_without_spaces() {
        let mut text = String::from("本アプリは");
        join(&mut text, "学習用です。");
        assert_eq!(text, "本アプリは学習用です。");
        let mut text = String::from("one");
        join(&mut text, "two");
        assert_eq!(text, "one two");
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
        for (doc, locale, _, text) in all_shown() {
            {
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
}
