//! Exam board material: questions from past exams, answer keys and
//! summaries of board handouts, each classified against the plan's topics.

use serde::{Deserialize, Deserializer};

use crate::AiError;
use crate::client::{ChatMessage, Client};
use crate::config::Task;
use crate::prompts::{self, fill};
use crate::tasks::chat_json;

/// Characters of document text per request.
const CHUNK_CHARS: usize = 12_000;
/// Overlap between chunks, so a question cut at a boundary is whole in one.
const CHUNK_OVERLAP: usize = 1_500;

/// The plan the material is checked against. Topics are numbered from 1 in
/// this order; the numbers in the model's answers refer to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardContext {
    pub exam: String,
    /// "Subject — Topic" per plan topic.
    pub topics: Vec<String>,
}

impl BoardContext {
    fn topics_list(&self) -> String {
        self.topics
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{}. {t}", i + 1))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Topic index (0-based) for a 1-based number from the model.
    pub fn topic_index(&self, number: Option<u32>) -> Option<usize> {
        let n = number? as usize;
        (1..=self.topics.len()).contains(&n).then(|| n - 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BoardQuestionDraft {
    #[serde(default, deserialize_with = "lenient_u32")]
    pub number: Option<u32>,
    pub prompt: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default, deserialize_with = "lenient_string")]
    pub answer: Option<String>,
    #[serde(default, deserialize_with = "lenient_u32")]
    pub topic: Option<u32>,
    #[serde(default)]
    pub relevance: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExcerptDraft {
    pub summary: String,
    #[serde(default, deserialize_with = "lenient_u32")]
    pub topic: Option<u32>,
    #[serde(default)]
    pub relevance: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AnswerDraft {
    #[serde(deserialize_with = "lenient_u32")]
    pub number: Option<u32>,
    #[serde(default, deserialize_with = "lenient_string")]
    pub answer: Option<String>,
}

#[derive(Deserialize)]
struct QuestionsReply {
    #[serde(default)]
    questions: Vec<BoardQuestionDraft>,
}

#[derive(Deserialize)]
struct ExcerptsReply {
    #[serde(default)]
    excerpts: Vec<ExcerptDraft>,
}

#[derive(Deserialize)]
struct AnswersReply {
    #[serde(default)]
    answers: Vec<AnswerDraft>,
}

/// Classification of an existing item against another plan's syllabus.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RelevanceDraft {
    pub item: usize,
    #[serde(default, deserialize_with = "lenient_u32")]
    pub topic: Option<u32>,
    pub relevance: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Deserialize)]
struct RelevanceReply {
    items: Vec<RelevanceDraft>,
}

/// Reclassifies existing content without rewriting questions, options or answers.
pub fn classify_board_items(
    client: &Client,
    context: &BoardContext,
    items: &[String],
) -> Result<Vec<RelevanceDraft>, AiError> {
    let topics = context.topics_list();
    let values = items
        .iter()
        .enumerate()
        .map(|(index, text)| serde_json::json!({"item": index, "text": text}))
        .collect::<Vec<_>>();
    let messages = [
        ChatMessage::system(fill(
            prompts::BOARD_RELEVANCE,
            &[("exam", &context.exam), ("topics", &topics)],
        )),
        ChatMessage::user(serde_json::Value::Array(values).to_string()),
    ];
    let reply: RelevanceReply = chat_json(client, Task::StudyPlan, &messages, 8_000)?;
    Ok(reply.items)
}

/// Numbers may come as `12`, `"12"` or `null`.
fn lenient_u32<'de, D: Deserializer<'de>>(de: D) -> Result<Option<u32>, D::Error> {
    Ok(match serde_json::Value::deserialize(de)? {
        serde_json::Value::Number(n) => n.as_u64().and_then(|n| u32::try_from(n).ok()),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    })
}

fn lenient_string<'de, D: Deserializer<'de>>(de: D) -> Result<Option<String>, D::Error> {
    Ok(match serde_json::Value::deserialize(de)? {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

/// Splits long text into overlapping chunks at line boundaries.
pub fn chunk_text(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= CHUNK_CHARS {
        return vec![text.to_owned()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let mut end = (start + CHUNK_CHARS).min(chars.len());
        // Prefer to cut at a line break in the last fifth of the chunk.
        if end < chars.len()
            && let Some(nl) = chars[start + CHUNK_CHARS * 4 / 5..end]
                .iter()
                .rposition(|c| *c == '\n')
        {
            end = start + CHUNK_CHARS * 4 / 5 + nl + 1;
        }
        chunks.push(chars[start..end].iter().collect());
        if end == chars.len() {
            break;
        }
        start = end.saturating_sub(CHUNK_OVERLAP).max(start + 1);
    }
    chunks
}

/// Questions found in one chunk of a past exam.
pub fn extract_board_questions(
    client: &Client,
    context: &BoardContext,
    chunk: &str,
) -> Result<Vec<BoardQuestionDraft>, AiError> {
    let topics = context.topics_list();
    let messages = [
        ChatMessage::system(fill(
            prompts::BOARD_EXAM,
            &[("exam", &context.exam), ("topics", &topics)],
        )),
        ChatMessage::user(format!("TRECHO DA PROVA:\n{chunk}")),
    ];
    let reply: QuestionsReply = chat_json(client, Task::StudyPlan, &messages, 16_000)?;
    Ok(reply.questions)
}

/// Summaries of board material in one chunk.
pub fn extract_board_excerpts(
    client: &Client,
    context: &BoardContext,
    chunk: &str,
) -> Result<Vec<ExcerptDraft>, AiError> {
    let topics = context.topics_list();
    let messages = [
        ChatMessage::system(fill(
            prompts::BOARD_MATERIAL,
            &[("exam", &context.exam), ("topics", &topics)],
        )),
        ChatMessage::user(format!("TRECHO DO MATERIAL:\n{chunk}")),
    ];
    let reply: ExcerptsReply = chat_json(client, Task::StudyPlan, &messages, 12_000)?;
    Ok(reply.excerpts)
}

/// Answers listed in one chunk of an answer key.
pub fn extract_answer_key(
    client: &Client,
    exam: &str,
    chunk: &str,
) -> Result<Vec<AnswerDraft>, AiError> {
    let messages = [
        ChatMessage::system(fill(prompts::BOARD_KEY, &[("exam", exam)])),
        ChatMessage::user(format!("GABARITO:\n{chunk}")),
    ];
    let reply: AnswersReply = chat_json(client, Task::Quiz, &messages, 8_000)?;
    Ok(reply.answers)
}

/// Prompt block with board material about a topic, or an empty string.
/// `questions` are `(statement, answer)` pairs.
pub fn reference_block(excerpts: &[String], questions: &[(String, String)]) -> String {
    if excerpts.is_empty() && questions.is_empty() {
        return String::new();
    }
    let list = |items: Vec<String>| {
        if items.is_empty() {
            "-".to_owned()
        } else {
            items.join("\n")
        }
    };
    let excerpts = list(excerpts.iter().map(|e| format!("- {e}")).collect());
    let questions = list(
        questions
            .iter()
            .map(|(q, a)| format!("- {q} → {a}"))
            .collect(),
    );
    fill(
        prompts::REFERENCE,
        &[("excerpts", &excerpts), ("questions", &questions)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lenient_numbers() {
        let reply: QuestionsReply = serde_json::from_str(
            r#"{"questions":[{"number":"12","prompt":"P","options":["Certo","Errado"],
                "answer":null,"topic":3,"relevance":"relevant","reason":"r"},
               {"prompt":"Q","topic":"x"}]}"#,
        )
        .unwrap();
        assert_eq!(reply.questions[0].number, Some(12));
        assert_eq!(reply.questions[0].topic, Some(3));
        assert_eq!(reply.questions[1].topic, None);
        assert_eq!(reply.questions[1].answer, None);
    }

    #[test]
    fn topic_numbers_map_to_indices() {
        let ctx = BoardContext {
            exam: "TJCE".into(),
            topics: vec!["Português — Crase".into(), "Português — Regência".into()],
        };
        assert_eq!(ctx.topic_index(Some(2)), Some(1));
        assert_eq!(ctx.topic_index(Some(3)), None);
        assert_eq!(ctx.topic_index(Some(0)), None);
        assert!(ctx.topics_list().starts_with("1. Português — Crase\n2."));
    }

    #[test]
    fn chunks_overlap_and_cover_everything() {
        let text: String = (0..3_000).map(|i| format!("linha {i}\n")).collect();
        let chunks = chunk_text(&text);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.chars().count() <= CHUNK_CHARS));
        assert!(chunks[0].ends_with('\n'));
        assert!(chunks.last().unwrap().ends_with("linha 2999\n"));
    }

    #[test]
    fn reference_block_is_empty_without_material() {
        assert!(reference_block(&[], &[]).is_empty());
        let block = reference_block(&["Crase é fusão.".into()], &[]);
        assert!(block.contains("- Crase é fusão."));
    }
}
