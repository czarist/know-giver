//! Second pass over a generated handout: a content review that fixes wrong
//! statements, then a blind solve of every exercise. An exercise is kept only
//! when the independent answer matches its answer key.
//!
//! The reviewer returns only its corrections (exact excerpt -> fixed text, and
//! exercise key changes), not the whole handout again: rewriting everything
//! took as long as writing the handout.

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

#[derive(Deserialize, Default)]
struct ReviewReply {
    #[serde(default)]
    corrections: Vec<Correction>,
    #[serde(default)]
    exercises: Vec<ExerciseFix>,
    #[serde(default)]
    issues: Vec<String>,
}

/// Replace `original` (an exact excerpt) with `corrected`; empty removes it.
#[derive(Deserialize, Default)]
struct Correction {
    #[serde(default)]
    original: String,
    #[serde(default)]
    corrected: String,
}

/// A change to exercise `n` (1-based): a new answer key, or removal.
#[derive(Deserialize, Default)]
struct ExerciseFix {
    #[serde(default)]
    n: serde_json::Value,
    #[serde(default)]
    correct: serde_json::Value,
    #[serde(default)]
    remove: bool,
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
        ChatMessage::system(fill(
            prompts::get(client.language()).handout_review,
            &topic.values(),
        )),
        ChatMessage::user(format!(
            "{}\n{edital}\n\n{}\n{json}",
            client.language().pick(
                "EDITAL (dados de referência):",
                "EXAM NOTICE (reference data):"
            ),
            client
                .language()
                .pick("APOSTILA (dados a revisar):", "HANDOUT (data to review):")
        )),
    ];
    let reply: ReviewReply = chat_json(client, Task::Correction, &messages, 6_000)?;
    let (mut reviewed, issues) = apply_review(draft, reply);

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
/// Applies the reviewer's corrections. Excerpts that are not found verbatim
/// are skipped (and logged) rather than guessed.
fn apply_review(mut draft: HandoutDraft, reply: ReviewReply) -> (HandoutDraft, Vec<String>) {
    let mut missed = 0;
    for fix in &reply.corrections {
        if fix.original.trim().is_empty() || fix.original == fix.corrected {
            continue;
        }
        if !replace_once(&mut draft, &fix.original, &fix.corrected) {
            missed += 1;
            tracing::debug!(original = %fix.original, "review excerpt not found");
        }
    }
    if missed > 0 {
        tracing::info!(missed, "some review corrections did not match the handout");
    }

    let mut removed = vec![false; draft.exercises.len()];
    for fix in &reply.exercises {
        let Some(index) = as_index(&fix.n)
            .and_then(|n| n.checked_sub(1))
            .filter(|i| *i < draft.exercises.len())
        else {
            continue;
        };
        if fix.remove {
            removed[index] = true;
        } else if let Some(correct) =
            as_index(&fix.correct).filter(|c| *c < draft.exercises[index].options.len())
        {
            draft.exercises[index].correct = correct;
        }
    }
    let mut flags = removed.into_iter();
    draft.exercises.retain(|_| !flags.next().unwrap_or(false));
    prune_empty(&mut draft);
    (draft, reply.issues)
}

/// Replaces the first exact occurrence of `original` in any text of the
/// handout. Returns false when it appears nowhere.
fn replace_once(draft: &mut HandoutDraft, original: &str, corrected: &str) -> bool {
    let mut texts: Vec<&mut String> = vec![&mut draft.introduction];
    for section in &mut draft.sections {
        texts.push(&mut section.heading);
        texts.extend(section.paragraphs.iter_mut());
        texts.extend(section.key_points.iter_mut());
    }
    texts.extend(draft.examples.iter_mut());
    texts.extend(draft.pitfalls.iter_mut());
    texts.extend(draft.summary.iter_mut());
    for exercise in &mut draft.exercises {
        texts.push(&mut exercise.prompt);
        texts.extend(exercise.options.iter_mut());
        texts.push(&mut exercise.explanation);
    }
    for text in texts {
        if let Some(at) = text.find(original) {
            text.replace_range(at..at + original.len(), corrected);
            *text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            return true;
        }
    }
    false
}

/// Drops list items a correction emptied. Exercises keep their option count
/// so answer keys stay valid.
fn prune_empty(draft: &mut HandoutDraft) {
    let keep = |t: &String| !t.trim().is_empty();
    for section in &mut draft.sections {
        section.paragraphs.retain(keep);
        section.key_points.retain(keep);
    }
    draft.examples.retain(keep);
    draft.pitfalls.retain(keep);
    draft.summary.retain(keep);
}

fn solve_blind(
    client: &Client,
    topic: &TopicContext,
    exercises: &[QuestionDraft],
) -> Result<Vec<(usize, Option<usize>)>, AiError> {
    let messages = [
        ChatMessage::system(fill(
            prompts::get(client.language()).exercise_check,
            &topic.values(),
        )),
        ChatMessage::user(blind_questions(exercises)),
    ];
    let reply: BlindReply = chat_json(client, Task::Verify, &messages, 8_000)?;
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
            r#"{"corrections":[{"original":"a","corrected":"b"}],"exercises":[{"n":"1","correct":2}],"issues":["corrigido x"]}"#,
        )
        .unwrap();
        assert_eq!(reply.issues, vec!["corrigido x"]);
        assert_eq!(reply.corrections.len(), 1);
        assert_eq!(as_index(&reply.exercises[0].n), Some(1));
        let blind: BlindReply =
            serde_json::from_str(r#"{"answers":[{"n":"2","answer":1},{"n":3,"answer":null}]}"#)
                .unwrap();
        assert_eq!(as_index(&blind.answers[0].n), Some(2));
        assert_eq!(as_index(&blind.answers[1].answer), None);
    }

    fn sample() -> HandoutDraft {
        HandoutDraft {
            introduction: "Crase é a fusão de a + a.".into(),
            sections: vec![SectionDraft {
                heading: "Regra".into(),
                paragraphs: vec!["Usa-se crase antes de meio-dia? Não.".into()],
                key_points: vec!["Ponto errado.".into(), "Ponto certo.".into()],
                image: String::new(),
            }],
            exercises: vec![question(0), question(1), question(2)],
            ..HandoutDraft::default()
        }
    }

    #[test]
    fn corrections_replace_exact_excerpts_and_remove_emptied_items() {
        let reply = ReviewReply {
            corrections: vec![
                Correction {
                    original: "Não.".into(),
                    corrected: "Sim: ao meio-dia.".into(),
                },
                Correction {
                    original: "Ponto errado.".into(),
                    corrected: String::new(),
                },
                Correction {
                    original: "texto que não existe".into(),
                    corrected: "x".into(),
                },
            ],
            ..ReviewReply::default()
        };
        let (draft, _) = apply_review(sample(), reply);
        let section = &draft.sections[0];
        assert_eq!(
            section.paragraphs[0],
            "Usa-se crase antes de meio-dia? Sim: ao meio-dia."
        );
        assert_eq!(section.key_points, vec!["Ponto certo."]);
        assert_eq!(draft.introduction, "Crase é a fusão de a + a.");
    }

    #[test]
    fn exercise_fixes_change_keys_and_remove_questions() {
        let reply = ReviewReply {
            exercises: vec![
                ExerciseFix {
                    n: serde_json::json!(1),
                    correct: serde_json::json!(3),
                    remove: false,
                },
                ExerciseFix {
                    n: serde_json::json!(2),
                    remove: true,
                    ..ExerciseFix::default()
                },
                // Out of range: ignored.
                ExerciseFix {
                    n: serde_json::json!(3),
                    correct: serde_json::json!(9),
                    remove: false,
                },
                ExerciseFix {
                    n: serde_json::json!(7),
                    remove: true,
                    ..ExerciseFix::default()
                },
            ],
            ..ReviewReply::default()
        };
        let (draft, _) = apply_review(sample(), reply);
        let keys: Vec<usize> = draft.exercises.iter().map(|q| q.correct).collect();
        assert_eq!(keys, vec![3, 2]);
    }
}
