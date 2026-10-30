//! Prompt templates, embedded from `crates/ai/prompts/<language>/`. Each
//! language has the same files with the same placeholders, JSON keys and
//! line markers (`QUADRO:`, `ROTEIRO:`, `[ilegível]`…), so replies are read
//! the same way whatever the language.

use crate::config::PromptLanguage;

/// Every prompt of one language.
#[derive(Debug)]
pub struct Prompts {
    pub plan: &'static str,
    pub roles: &'static str,
    pub tutor: &'static str,
    pub lesson_start: &'static str,
    pub quiz: &'static str,
    pub review_question: &'static str,
    pub review_eval: &'static str,
    /// Handout instructions, also used to invalidate cached text when the prompt changes.
    pub handout: &'static str,
    pub handout_review: &'static str,
    pub exercise_check: &'static str,
    pub board_exam: &'static str,
    pub board_key: &'static str,
    pub board_material: &'static str,
    pub board_relevance: &'static str,
    pub reference: &'static str,
    pub discursive_proposal: &'static str,
    pub discursive_transcribe: &'static str,
    pub discursive_evaluate: &'static str,
    pub discursive_difficulties: &'static str,
}

macro_rules! prompts {
    ($dir:literal) => {
        Prompts {
            plan: include_str!(concat!("../prompts/", $dir, "/plan.md")),
            roles: include_str!(concat!("../prompts/", $dir, "/roles.md")),
            tutor: include_str!(concat!("../prompts/", $dir, "/tutor.md")),
            lesson_start: include_str!(concat!("../prompts/", $dir, "/lesson_start.md")),
            quiz: include_str!(concat!("../prompts/", $dir, "/quiz.md")),
            review_question: include_str!(concat!("../prompts/", $dir, "/review_question.md")),
            review_eval: include_str!(concat!("../prompts/", $dir, "/review_eval.md")),
            handout: include_str!(concat!("../prompts/", $dir, "/handout.md")),
            handout_review: include_str!(concat!("../prompts/", $dir, "/handout_review.md")),
            exercise_check: include_str!(concat!("../prompts/", $dir, "/exercise_check.md")),
            board_exam: include_str!(concat!("../prompts/", $dir, "/board_exam.md")),
            board_key: include_str!(concat!("../prompts/", $dir, "/board_key.md")),
            board_material: include_str!(concat!("../prompts/", $dir, "/board_material.md")),
            board_relevance: include_str!(concat!("../prompts/", $dir, "/board_relevance.md")),
            reference: include_str!(concat!("../prompts/", $dir, "/reference.md")),
            discursive_proposal: include_str!(concat!(
                "../prompts/",
                $dir,
                "/discursive_proposal.md"
            )),
            discursive_transcribe: include_str!(concat!(
                "../prompts/",
                $dir,
                "/discursive_transcribe.md"
            )),
            discursive_evaluate: include_str!(concat!(
                "../prompts/",
                $dir,
                "/discursive_evaluate.md"
            )),
            discursive_difficulties: include_str!(concat!(
                "../prompts/",
                $dir,
                "/discursive_difficulties.md"
            )),
        }
    };
}

static PORTUGUESE: Prompts = prompts!("pt");
static ENGLISH: Prompts = prompts!("en");
static JAPANESE: Prompts = prompts!("ja");
static CHINESE_SIMPLIFIED: Prompts = prompts!("zh-Hans");
static CHINESE_TRADITIONAL: Prompts = prompts!("zh-Hant");

/// The prompts in `language`.
pub fn get(language: PromptLanguage) -> &'static Prompts {
    match language {
        PromptLanguage::Portuguese => &PORTUGUESE,
        PromptLanguage::English => &ENGLISH,
        PromptLanguage::Japanese => &JAPANESE,
        PromptLanguage::ChineseSimplified => &CHINESE_SIMPLIFIED,
        PromptLanguage::ChineseTraditional => &CHINESE_TRADITIONAL,
    }
}

/// Every prompt a handout goes through. Cached handouts store it, so any
/// prompt change (or another language) makes old handouts stale.
pub fn handout_pipeline(language: PromptLanguage) -> String {
    let p = get(language);
    [p.handout, p.handout_review, p.exercise_check].join(
        "
---
",
    )
}

/// Replaces `{name}` placeholders. Unknown placeholders are left as they are.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    values
        .iter()
        .fold(template.to_owned(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn fills_placeholders_and_keeps_json_braces() {
        let out = fill(
            get(PromptLanguage::Portuguese).quiz,
            &[
                ("exam", "TRF"),
                ("subject", "Português"),
                ("topic", "Crase"),
                ("count", "5"),
            ],
        );
        assert!(out.contains("Tópico: Crase"));
        assert!(out.contains("exatamente 5 questões"));
        assert!(out.contains("{\"questions\": ["));
        assert!(!out.contains("{topic}"));
    }

    /// `{name}` placeholders of a template (JSON braces have quotes or spaces).
    fn placeholders(template: &str) -> BTreeSet<String> {
        template
            .split('{')
            .skip(1)
            .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
            .filter(|name| {
                !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
            })
            .map(str::to_owned)
            .collect()
    }

    /// The JSON keys a template asks for, in its JSON example lines.
    fn json_keys(template: &str) -> BTreeSet<String> {
        let json: String = template
            .lines()
            .filter(|l| l.trim_start().starts_with('{') || l.contains("JSON: {"))
            .collect();
        json.split('"')
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|w| w[1].starts_with(':'))
            .map(|w| w[0].to_owned())
            .collect()
    }

    const ALL: [PromptLanguage; 5] = [
        PromptLanguage::Portuguese,
        PromptLanguage::English,
        PromptLanguage::Japanese,
        PromptLanguage::ChineseSimplified,
        PromptLanguage::ChineseTraditional,
    ];

    #[test]
    fn every_language_has_the_same_placeholders_keys_and_markers() {
        for lang in ALL {
            same_as_portuguese(lang);
        }
        let pipelines: BTreeSet<String> = ALL.iter().map(|l| handout_pipeline(*l)).collect();
        assert_eq!(
            pipelines.len(),
            ALL.len(),
            "each language has its own handout cache key"
        );
    }

    fn same_as_portuguese(lang: PromptLanguage) {
        let (pt, en) = (get(PromptLanguage::Portuguese), get(lang));
        let pairs = [
            (pt.plan, en.plan),
            (pt.roles, en.roles),
            (pt.tutor, en.tutor),
            (pt.lesson_start, en.lesson_start),
            (pt.quiz, en.quiz),
            (pt.review_question, en.review_question),
            (pt.review_eval, en.review_eval),
            (pt.handout, en.handout),
            (pt.handout_review, en.handout_review),
            (pt.exercise_check, en.exercise_check),
            (pt.board_exam, en.board_exam),
            (pt.board_key, en.board_key),
            (pt.board_material, en.board_material),
            (pt.board_relevance, en.board_relevance),
            (pt.reference, en.reference),
            (pt.discursive_proposal, en.discursive_proposal),
            (pt.discursive_transcribe, en.discursive_transcribe),
            (pt.discursive_evaluate, en.discursive_evaluate),
            (pt.discursive_difficulties, en.discursive_difficulties),
        ];
        for (pt, en) in pairs {
            assert_eq!(placeholders(pt), placeholders(en), "{en}");
            assert_eq!(json_keys(pt), json_keys(en), "{en}");
        }
        for marker in [
            "QUADRO:",
            "FORMULA:",
            "DIAGRAMA:",
            "TABELA:",
            "LINHA DO TEMPO:",
            "MAPA:",
            "DESTAQUE:",
            "EXEMPLO:",
            "IMAGEM:",
        ] {
            assert!(en.tutor.contains(marker), "{marker}");
        }
        for mark in ["[?", "[ilegível]", "[rasura"] {
            assert!(en.discursive_transcribe.contains(mark), "{mark}");
        }
        for marker in ["ROTEIRO:", "ETAPA:", "FIM"] {
            assert_eq!(
                pt.tutor.contains(marker) || pt.lesson_start.contains(marker),
                en.tutor.contains(marker) || en.lesson_start.contains(marker),
                "{lang:?} {marker}"
            );
        }
    }
}
