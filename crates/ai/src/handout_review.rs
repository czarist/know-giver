//! Second pass over a generated handout: a content review that fixes wrong
//! statements, then a blind solve of every exercise. An exercise is kept only
//! when the independent answer matches its answer key.

use serde::Deserialize;

use crate::AiError;
use crate::client::{ChatMessage, Client};
use crate::config::Task;
use crate::prompts::{self, fill};
use crate::tasks::{HandoutDraft, QuestionDraft, TopicContext, chat_json};

/// A handout after review, with what changed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReviewedHandout {
    pub draft: HandoutDraft,
    /// Corrections reported by the reviewer, one sentence each.
    pub issues: Vec<String>,
    /// Exercises removed because the blind solve disagreed with the key.
    pub dropped_exercises: usize,
}

#[derive(Deserialize)]
struct ReviewReply {
    #[serde(flatten)]
    draft: HandoutDraft,
    #[serde(default)]
    issues: Vec<String>,
}

#[derive(Deserialize)]
struct BlindReply {
    #[serde(default)]
    answers: Vec<BlindAnswer>,
}

#[derive(Deserialize)]
struct BlindAnswer {
    #[serde(default)]
    n: serde_json::Value,
    #[serde(default)]
    answer: serde_json::Value,
}

/// Number from `1`, `"1"` or nothing.
fn as_index(value: &serde_json::Value) -> Option<usize> {
    match value {
        serde_json::Value::Number(n) => n.as_u64().and_then(|n| usize::try_from(n).ok()),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Reviews `draft` and checks its exercises. `edital` is the same reference
/// excerpt the handout was written from.
pub fn review_handout(
    client: &Client,
    topic: &TopicContext,
    edital: &str,
    draft: HandoutDraft,
) -> Result<ReviewedHandout, AiError> {
    let json = serde_json::to_string(&draft)
        .map_err(|e| AiError::InvalidResponse(format!("handout not serializable: {e}")))?;
    let messages = [
        ChatMessage::system(fill(prompts::HANDOUT_REVIEW, &topic.values())),
        ChatMessage::user(format!(
            "EDITAL (dados de referência):\n{edital}\n\nAPOSTILA (dados a revisar):\n{json}"
        )),
    ];
    let reply: ReviewReply = chat_json(client, Task::Correction, &messages, 16_000)?;
    let (mut reviewed, issues) = accept_review(draft, reply);

    let before = reviewed.exercises.len();
    if before > 0 {
        let answers = solve_blind(client, topic, &reviewed.exercises)?;
        reviewed.exercises = keep_verified(reviewed.exercises, &answers);
    }
    let dropped_exercises = before - reviewed.exercises.len();
    tracing::info!(
        topic = %topic.topic,
        issues = issues.len(),
        dropped_exercises,
        "handout reviewed"
    );
    for issue in &issues {
        tracing::debug!(topic = %topic.topic, %issue, "handout correction");
    }
    Ok(ReviewedHandout {
        draft: reviewed,
        issues,
        dropped_exercises,
    })
}

/// Uses the reviewed text unless the reviewer lost the content, in which
/// case the original is kept (its exercises are still checked afterwards).
fn accept_review(original: HandoutDraft, reply: ReviewReply) -> (HandoutDraft, Vec<String>) {
    let lost_content = (reply.draft.sections.is_empty() && !original.sections.is_empty())
        || (reply.draft.introduction.trim().is_empty() && !original.introduction.trim().is_empty());
    if lost_content {
        tracing::warn!("handout review returned an incomplete handout; keeping the original");
        (original, Vec::new())
    } else {
        (reply.draft, reply.issues)
    }
}

/// Independent answers for `exercises`: `(1-based number, option index)`.
fn solve_blind(
    client: &Client,
    topic: &TopicContext,
    exercises: &[QuestionDraft],
) -> Result<Vec<(usize, Option<usize>)>, AiError> {
    let messages = [
        ChatMessage::system(fill(prompts::EXERCISE_CHECK, &topic.values())),
        ChatMessage::user(blind_questions(exercises)),
    ];
    let reply: BlindReply = chat_json(client, Task::Correction, &messages, 8_000)?;
    Ok(reply
        .answers
        .iter()
        .filter_map(|a| Some((as_index(&a.n)?, as_index(&a.answer))))
        .collect())
}

/// Exercises without their key, options numbered from 0.
fn blind_questions(exercises: &[QuestionDraft]) -> String {
    exercises
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let options = q
                .options
                .iter()
                .enumerate()
                .map(|(j, o)| format!("   ({j}) {o}"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{}. {}\n{options}", i + 1, q.prompt)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Keeps exercise `i` only if answer `i + 1` exists and equals its key.
fn keep_verified(
    exercises: Vec<QuestionDraft>,
    answers: &[(usize, Option<usize>)],
) -> Vec<QuestionDraft> {
    exercises
        .into_iter()
        .enumerate()
        .filter(|(i, q)| {
            answers
                .iter()
                .any(|(n, answer)| *n == i + 1 && *answer == Some(q.correct))
        })
        .map(|(_, q)| q)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::SectionDraft;

    fn question(correct: usize) -> QuestionDraft {
        QuestionDraft {
            prompt: "Qual?".into(),
            options: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            correct,
            explanation: String::new(),
        }
    }

    #[test]
    fn keeps_only_exercises_confirmed_by_blind_solve() {
        let exercises = vec![question(0), question(2), question(1), question(3)];
        let answers = [(1, Some(0)), (2, Some(1)), (3, None)];
        let kept = keep_verified(exercises, &answers);
        // 1 matches; 2 disagrees; 3 ambiguous; 4 unanswered.
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].correct, 0);
    }

    #[test]
    fn blind_questions_hide_the_key() {
        let text = blind_questions(&[question(2)]);
        assert!(text.starts_with("1. Qual?"));
        assert!(text.contains("(3) d"));
        assert!(!text.contains("correct"));
    }

    #[test]
    fn review_reply_parses_with_issues_and_lenient_numbers() {
        let reply: ReviewReply = serde_json::from_str(
            r#"{"introduction":"I","sections":[{"heading":"H"}],"issues":["corrigido x"]}"#,
        )
        .unwrap();
        assert_eq!(reply.issues, vec!["corrigido x"]);
        let blind: BlindReply =
            serde_json::from_str(r#"{"answers":[{"n":"2","answer":1},{"n":3,"answer":null}]}"#)
                .unwrap();
        assert_eq!(as_index(&blind.answers[0].n), Some(2));
        assert_eq!(as_index(&blind.answers[1].answer), None);
    }

    #[test]
    fn incomplete_review_keeps_the_original() {
        let original = HandoutDraft {
            introduction: "Intro".into(),
            sections: vec![SectionDraft {
                heading: "Regra".into(),
                ..SectionDraft::default()
            }],
            ..HandoutDraft::default()
        };
        let reply = ReviewReply {
            draft: HandoutDraft::default(),
            issues: vec!["x".into()],
        };
        let (kept, issues) = accept_review(original.clone(), reply);
        assert_eq!(kept, original);
        assert!(issues.is_empty());
    }
}
