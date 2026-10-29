//! English passages inside the tutor's Portuguese speech.
//!
//! The tutor writes them between `[en]` and `[/en]`. They are read in an
//! English voice when one is chosen, and the marks never reach the screen.

use crate::split_for_speech;

const OPEN: &str = "[en]";
const CLOSE: &str = "[/en]";
/// Characters dropped from English passages before speaking (Markdown).
const MARKUP: [char; 6] = ['*', '#', '`', '_', '>', '|'];

/// The language a piece of speech is read in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechLang {
    /// The main voice (Portuguese).
    Main,
    English,
}

/// Splits marked text into runs of each language. Speech is split into
/// sentences before it is spoken, which can cut a passage in two: a closing
/// mark with no opening one makes the text before it English, and an opening
/// mark with no closing one makes the rest English.
pub fn language_runs(text: &str) -> Vec<(SpeechLang, String)> {
    let lower = text.to_ascii_lowercase();
    let mut marks: Vec<(usize, bool)> = lower
        .match_indices(OPEN)
        .map(|(i, _)| (i, true))
        .chain(lower.match_indices(CLOSE).map(|(i, _)| (i, false)))
        .collect();
    marks.sort_unstable();
    let mut lang = match marks.first() {
        Some((_, false)) => SpeechLang::English,
        _ => SpeechLang::Main,
    };
    let mut runs: Vec<(SpeechLang, String)> = Vec::new();
    let mut push = |lang: SpeechLang, piece: &str| {
        if piece.trim().is_empty() {
            return;
        }
        match runs.last_mut() {
            Some((last, run)) if *last == lang => run.push_str(piece),
            _ => runs.push((lang, piece.to_owned())),
        }
    };
    let mut from = 0;
    for (at, open) in marks {
        push(lang, &text[from..at]);
        from = at + if open { OPEN.len() } else { CLOSE.len() };
        lang = if open {
            SpeechLang::English
        } else {
            SpeechLang::Main
        };
    }
    push(lang, &text[from..]);
    runs.into_iter()
        .map(|(lang, run)| (lang, run.trim().to_owned()))
        .collect()
}

/// The text without language marks, for captions and the transcript.
pub fn strip_language_marks(text: &str) -> String {
    if !text.contains('[') {
        return text.to_owned();
    }
    language_runs(text)
        .into_iter()
        .map(|(_, run)| run)
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" ,", ",")
        .replace(" .", ".")
}

/// Pieces to synthesize, in order. English passages get their own pieces
/// when `english` is true; otherwise everything is read by the main voice.
pub fn speech_pieces(text: &str, english: bool) -> Vec<(SpeechLang, String)> {
    let mut pieces = Vec::new();
    for (lang, run) in language_runs(text) {
        if lang == SpeechLang::English && english {
            let clean: String = run.chars().filter(|c| !MARKUP.contains(c)).collect();
            let clean = clean.trim();
            if clean.chars().any(char::is_alphanumeric) {
                pieces.push((SpeechLang::English, clean.to_owned()));
            }
        } else {
            pieces.extend(
                split_for_speech(&run)
                    .into_iter()
                    .filter(|chunk| chunk.chars().any(char::is_alphanumeric))
                    .map(|chunk| (SpeechLang::Main, chunk)),
            );
        }
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;
    use SpeechLang::{English, Main};

    fn runs(text: &str) -> Vec<(SpeechLang, String)> {
        language_runs(text)
    }

    #[test]
    fn finds_english_passages() {
        assert_eq!(
            runs("A frase [en]I have been working[/en] usa o present perfect."),
            vec![
                (Main, "A frase".into()),
                (English, "I have been working".into()),
                (Main, "usa o present perfect.".into()),
            ]
        );
        assert_eq!(
            runs("Sem inglês aqui."),
            vec![(Main, "Sem inglês aqui.".into())]
        );
        assert_eq!(runs("[EN]Hello![/EN]"), vec![(English, "Hello!".into())]);
    }

    #[test]
    fn a_passage_cut_between_sentences_stays_english() {
        assert_eq!(
            runs("Repita: [en]The book is on the table."),
            vec![
                (Main, "Repita:".into()),
                (English, "The book is on the table.".into())
            ]
        );
        assert_eq!(
            runs("It is red.[/en] Muito bem."),
            vec![(English, "It is red.".into()), (Main, "Muito bem.".into())]
        );
    }

    #[test]
    fn captions_lose_the_marks() {
        assert_eq!(
            strip_language_marks("Diga [en]good morning[/en], por favor."),
            "Diga good morning, por favor."
        );
        assert_eq!(strip_language_marks("Nada a tirar."), "Nada a tirar.");
    }

    #[test]
    fn without_an_english_voice_everything_is_main() {
        let text = "Diga [en]good morning[/en].";
        assert!(speech_pieces(text, false).iter().all(|(l, _)| *l == Main));
        let pieces = speech_pieces(text, true);
        assert_eq!(pieces[1], (English, "good morning".into()));
    }
}
