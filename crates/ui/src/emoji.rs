//! Colored emoji. egui cannot draw color fonts, so the common emoji are
//! drawn as Twemoji pictures (CC-BY 4.0, `assets/emoji`); others fall back
//! to the one-color emoji fonts (see `theme::fonts`).

use egui::{Color32, Image, vec2};
use rust_i18n::t;

use crate::emoji_set::{PICKER_LEN, SET};

const VARIATION: char = '\u{FE0F}';

/// Reactions offered under the tutor's turns, as in Teams.
pub const REACTIONS: [&str; 6] = ["👍", "❤️", "😂", "😮", "😕", "🙏"];

/// The picture of `emoji`, when it is in the set.
fn picture_of(emoji: &str, size: f32) -> Option<Image<'static>> {
    at_start(emoji).and_then(|(index, _)| picture(index, size))
}

/// Under a tutor turn: the student's reaction and a button to pick one.
/// Returns the new choice when it changed (`Some(None)` takes it back).
pub fn reactions(ui: &mut egui::Ui, current: Option<&str>) -> Option<Option<String>> {
    let mut changed = None;
    ui.horizontal(|ui| {
        if let Some(emoji) = current {
            let chip = match picture_of(emoji, 16.0) {
                Some(image) => egui::Button::image(image),
                None => egui::Button::new(emoji),
            }
            .corner_radius(10.0);
            if ui
                .add(chip)
                .on_hover_text(t!("lesson.reaction_remove"))
                .clicked()
            {
                changed = Some(None);
            }
        }
        let add = match picture_of("🙂", 14.0) {
            Some(image) => egui::Button::image(image.tint(Color32::from_white_alpha(120))),
            None => egui::Button::new("+"),
        }
        .frame(false);
        let add = ui.add(add).on_hover_text(t!("lesson.react"));
        egui::Popup::from_toggle_button_response(&add).show(|ui| {
            ui.horizontal(|ui| {
                for reaction in REACTIONS {
                    let Some(image) = picture_of(reaction, 24.0) else {
                        continue;
                    };
                    let button = egui::Button::image(image)
                        .frame(false)
                        .min_size(vec2(32.0, 32.0));
                    if ui.add(button).clicked() {
                        changed = Some((current != Some(reaction)).then(|| reaction.to_owned()));
                        ui.close();
                    }
                }
            });
        });
    });
    changed
}

/// The picture for one emoji of the set, `size` points tall.
fn picture(index: usize, size: f32) -> Option<Image<'static>> {
    let (_, bytes) = SET.get(index)?;
    Some(
        Image::from_bytes(format!("bytes://emoji/{index}.svg"), *bytes)
            .fit_to_exact_size(vec2(size, size)),
    )
}

/// The emoji of the set at the start of `text`, longest first: its index
/// and how many bytes it takes (with a trailing emoji-style mark).
fn at_start(text: &str) -> Option<(usize, usize)> {
    // Only the emoji that start with this character are tried: chat text is
    // checked every frame, character by character.
    let first = text.chars().next()?;
    if !could_start_emoji(first) {
        return None;
    }
    let candidates = by_first_char().get(&first)?;
    let mut best: Option<(usize, usize)> = None;
    for &index in candidates {
        let Some((emoji, _)) = SET.get(index) else {
            continue;
        };
        let plain = emoji.trim_end_matches(VARIATION);
        let len = if text.starts_with(emoji) {
            emoji.len()
        } else if text.starts_with(plain) {
            plain.len()
        } else {
            continue;
        };
        if best.is_none_or(|(_, longest)| len > longest) {
            best = Some((index, len));
        }
    }
    best.map(|(index, len)| {
        let extra = if text[len..].starts_with(VARIATION) {
            VARIATION.len_utf8()
        } else {
            0
        };
        (index, len + extra)
    })
}

/// Every emoji of the set starts at U+2000 or later; letters, accented ones
/// included, never do.
fn could_start_emoji(c: char) -> bool {
    u32::from(c) >= 0x2000
}

/// The set's emoji grouped by their first character, built once.
fn by_first_char() -> &'static std::collections::HashMap<char, Vec<usize>> {
    static INDEX: std::sync::OnceLock<std::collections::HashMap<char, Vec<usize>>> =
        std::sync::OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index: std::collections::HashMap<char, Vec<usize>> = Default::default();
        for (n, (emoji, _)) in SET.iter().enumerate() {
            if let Some(first) = emoji.chars().next() {
                index.entry(first).or_default().push(n);
            }
        }
        index
    })
}

/// A piece of text to draw: words or one emoji picture.
#[derive(Debug, PartialEq)]
enum Piece<'a> {
    Text(&'a str),
    Emoji(usize),
}

fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at < text.len() {
        if let Some((index, len)) = at_start(&text[at..]) {
            if start < at {
                out.push(Piece::Text(&text[start..at]));
            }
            out.push(Piece::Emoji(index));
            at += len;
            start = at;
        } else {
            at += text[at..].chars().next().map_or(1, char::len_utf8);
        }
    }
    if start < text.len() {
        out.push(Piece::Text(&text[start..]));
    }
    out
}

/// A wrapping label whose emoji are drawn in color.
pub fn label(ui: &mut egui::Ui, text: &str) {
    // Most messages have no emoji: drawn as before, without splitting.
    if !text.chars().any(could_start_emoji) {
        ui.label(text);
        return;
    }
    let pieces = pieces(text);
    if !pieces.iter().any(|p| matches!(p, Piece::Emoji(_))) {
        ui.label(text);
        return;
    }
    let size = ui.text_style_height(&egui::TextStyle::Body) * 1.15;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        for piece in pieces {
            match piece {
                // Word by word, so long messages still wrap.
                Piece::Text(text) => {
                    for word in text.split_inclusive(' ') {
                        ui.label(word.replace('\n', " "));
                    }
                }
                Piece::Emoji(index) => {
                    if let Some(image) = picture(index, size) {
                        ui.add(image);
                    }
                }
            }
        }
    });
}

/// The picker's button: 😊 in color.
pub fn picker_button() -> egui::Button<'static> {
    let index = SET.iter().position(|(e, _)| *e == "😊").unwrap_or(0);
    match picture(index, 20.0) {
        Some(image) => egui::Button::image(image),
        None => egui::Button::new("😊"),
    }
    .frame(false)
}

/// The picker grid; a click adds that emoji to `input`.
/// The most used first (the first [`PICKER_LEN`]), then the rest of the set,
/// in rows of [`PICKER_COLUMNS`] that scroll.
pub fn picker(ui: &mut egui::Ui, input: &mut String) {
    egui::ScrollArea::vertical()
        .id_salt("chat_emoji_scroll")
        .max_height(280.0)
        .show(ui, |ui| {
            egui::Grid::new("chat_emoji")
                .spacing([2.0, 2.0])
                .show(ui, |ui| {
                    for (index, (emoji, _)) in SET.iter().enumerate() {
                        let Some(image) = picture(index, 22.0) else {
                            continue;
                        };
                        let button = egui::Button::image(image)
                            .frame(false)
                            .min_size(vec2(30.0, 30.0));
                        if ui.add(button).clicked() {
                            input.push_str(emoji);
                        }
                        if index % PICKER_COLUMNS == PICKER_COLUMNS - 1 || index + 1 == PICKER_LEN {
                            ui.end_row();
                        }
                        // An empty row between the most used and the rest
                        // (a grid takes no `add_space`).
                        if index + 1 == PICKER_LEN {
                            ui.end_row();
                        }
                    }
                });
        });
}

/// Emoji per row in the picker.
const PICKER_COLUMNS: usize = 8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_are_found_with_or_without_the_style_mark() {
        let thumbs = SET.iter().position(|(e, _)| *e == "👍").unwrap();
        let peace = SET.iter().position(|(e, _)| *e == "✌️").unwrap();
        assert_eq!(
            pieces("Boa 👍 e ✌ e ✌️!"),
            [
                Piece::Text("Boa "),
                Piece::Emoji(thumbs),
                Piece::Text(" e "),
                Piece::Emoji(peace),
                Piece::Text(" e "),
                Piece::Emoji(peace),
                Piece::Text("!"),
            ]
        );
    }

    #[test]
    fn every_picker_emoji_has_a_picture() {
        assert!(PICKER_LEN <= SET.len());
        for (emoji, bytes) in &SET {
            assert!(
                std::str::from_utf8(bytes).is_ok_and(|svg| svg.contains("<svg")),
                "{emoji}"
            );
        }
    }
}
