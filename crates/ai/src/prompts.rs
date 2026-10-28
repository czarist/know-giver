//! Prompt templates, embedded from `crates/ai/prompts/`.

pub const PLAN: &str = include_str!("../prompts/plan.md");
pub const ROLES: &str = include_str!("../prompts/roles.md");
pub const TUTOR: &str = include_str!("../prompts/tutor.md");
pub const LESSON_START: &str = include_str!("../prompts/lesson_start.md");
pub const QUIZ: &str = include_str!("../prompts/quiz.md");
pub const REVIEW_QUESTION: &str = include_str!("../prompts/review_question.md");
pub const REVIEW_EVAL: &str = include_str!("../prompts/review_eval.md");
/// Handout instructions, also used to invalidate cached text when the prompt changes.
pub const HANDOUT: &str = include_str!("../prompts/handout.md");
pub const HANDOUT_REVIEW: &str = include_str!("../prompts/handout_review.md");
pub const EXERCISE_CHECK: &str = include_str!("../prompts/exercise_check.md");

/// Every prompt a handout goes through. Cached handouts store it, so any
/// prompt change makes old handouts stale.
pub fn handout_pipeline() -> String {
    [HANDOUT, HANDOUT_REVIEW, EXERCISE_CHECK].join(
        "
---
",
    )
}
pub const BOARD_EXAM: &str = include_str!("../prompts/board_exam.md");
pub const BOARD_KEY: &str = include_str!("../prompts/board_key.md");
pub const BOARD_MATERIAL: &str = include_str!("../prompts/board_material.md");
pub const BOARD_RELEVANCE: &str = include_str!("../prompts/board_relevance.md");
pub const REFERENCE: &str = include_str!("../prompts/reference.md");

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
    use super::*;

    #[test]
    fn fills_placeholders_and_keeps_json_braces() {
        let out = fill(
            QUIZ,
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
}
