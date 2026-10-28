//! Tutor tasks: plan generation, lessons, quizzes and review grading.

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::AiError;
use crate::client::{ChatMessage, ChatRequest, Client, Role};
use crate::config::Task;
use crate::prompts::{self, fill};

/// Longest tutor answer, in words. Voice answers must stay short.
const TUTOR_MAX_WORDS: &str = "120";

/// What the student told us about their preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPrefs {
    pub weeks: u32,
    pub hours_per_week: u32,
    /// Role (cargo) to focus on when the edital covers several.
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TopicDraft {
    pub title: String,
    #[serde(default)]
    pub difficulty: String,
    #[serde(default = "first_week")]
    pub week: u32,
}

fn first_week() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SubjectDraft {
    pub name: String,
    #[serde(default)]
    pub topics: Vec<TopicDraft>,
}

/// Plan as returned by the model, before validation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlanDraft {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub exam: String,
    #[serde(default)]
    pub subjects: Vec<SubjectDraft>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct QuestionDraft {
    pub prompt: String,
    pub options: Vec<String>,
    pub correct: usize,
    #[serde(default)]
    pub explanation: String,
}

#[derive(Deserialize)]
struct QuizDraft {
    questions: Vec<QuestionDraft>,
}

#[derive(Deserialize)]
struct ReviewQuestionDraft {
    question: String,
}

/// Correction of a review answer.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReviewFeedback {
    /// 0..=5, see `prompts/review_eval.md`.
    pub score: u8,
    pub feedback: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct SectionDraft {
    #[serde(default)]
    pub heading: String,
    #[serde(default)]
    pub paragraphs: Vec<String>,
    #[serde(default)]
    pub key_points: Vec<String>,
}

/// A written lesson (apostila) for one topic, before validation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct HandoutDraft {
    #[serde(default)]
    pub introduction: String,
    #[serde(default)]
    pub sections: Vec<SectionDraft>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub pitfalls: Vec<String>,
    #[serde(default)]
    pub summary: Vec<String>,
    #[serde(default)]
    pub exercises: Vec<QuestionDraft>,
}

/// Marker of the whiteboard line at the end of a tutor answer.
const BOARD_MARKER: &str = "QUADRO:";
/// Longest whiteboard point kept, in characters.
const BOARD_POINT_CHARS: usize = 90;

/// A tutor answer: what is spoken, and the key points for the whiteboard.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TutorReply {
    pub speech: String,
    pub board: Vec<String>,
}

impl TutorReply {
    /// Splits the "QUADRO: a | b" line off the end of the model's answer.
    pub fn parse(text: &str) -> Self {
        let mut speech = Vec::new();
        let mut board = Vec::new();
        for line in text.lines() {
            let clean = line.trim().trim_matches('*').trim();
            let is_marker = clean
                .get(..BOARD_MARKER.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(BOARD_MARKER));
            if is_marker {
                board = clean[BOARD_MARKER.len()..]
                    .split('|')
                    .map(|p| {
                        p.trim_matches(|c: char| c == '*' || c.is_whitespace())
                            .trim_end_matches('.')
                            .trim()
                    })
                    .filter(|p| !p.is_empty())
                    .map(|p| p.chars().take(BOARD_POINT_CHARS).collect())
                    .take(3)
                    .collect();
            } else {
                speech.push(line);
            }
        }
        Self {
            speech: speech.join("\n").trim().to_owned(),
            board,
        }
    }
}

/// Which topic a lesson, quiz or review is about.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TopicContext {
    pub exam: String,
    pub subject: String,
    pub topic: String,
    /// Board material about the topic (see [`crate::reference_block`]); may
    /// be empty.
    pub reference: String,
}

impl TopicContext {
    fn values(&self) -> [(&str, &str); 4] {
        [
            ("exam", self.exam.as_str()),
            ("subject", self.subject.as_str()),
            ("topic", self.topic.as_str()),
            ("reference", self.reference.as_str()),
        ]
    }
}

/// Generates a study plan from the edital text.
pub fn generate_plan(
    client: &Client,
    edital: &str,
    prefs: &PlanPrefs,
) -> Result<PlanDraft, AiError> {
    let role = if prefs.role.trim().is_empty() {
        "o cargo principal do edital"
    } else {
        prefs.role.trim()
    };
    let system = fill(
        prompts::PLAN,
        &[
            ("weeks", &prefs.weeks.to_string()),
            ("hours", &prefs.hours_per_week.to_string()),
            ("role", role),
        ],
    );
    let messages = [
        ChatMessage::system(system),
        ChatMessage::user(format!("EDITAL:\n{edital}")),
    ];
    chat_json(client, Task::StudyPlan, &messages, 32_000)
}

/// The tutor's opening message for a lesson.
pub fn start_lesson(client: &Client, topic: &TopicContext) -> Result<TutorReply, AiError> {
    let messages = [
        tutor_system(topic),
        ChatMessage::user(prompts::LESSON_START.trim()),
    ];
    client
        .chat(&ChatRequest {
            task: Task::Tutor,
            messages: &messages,
            json: false,
            max_tokens: 4_096,
        })
        .map(|text| TutorReply::parse(&text))
}

/// The tutor's next answer. `history` alternates student and tutor turns,
/// starting with the tutor's opening.
pub fn tutor_reply(
    client: &Client,
    topic: &TopicContext,
    history: &[ChatMessage],
) -> Result<TutorReply, AiError> {
    let mut messages = vec![
        tutor_system(topic),
        ChatMessage::user(prompts::LESSON_START.trim()),
    ];
    messages.extend(history.iter().filter(|m| m.role != Role::System).cloned());
    client
        .chat(&ChatRequest {
            task: Task::Tutor,
            messages: &messages,
            json: false,
            max_tokens: 4_096,
        })
        .map(|text| TutorReply::parse(&text))
}

fn tutor_system(topic: &TopicContext) -> ChatMessage {
    let mut values = topic.values().to_vec();
    values.push(("max_words", TUTOR_MAX_WORDS));
    ChatMessage::system(fill(prompts::TUTOR, &values))
}

/// Generates `count` multiple-choice questions about a topic.
pub fn generate_quiz(
    client: &Client,
    topic: &TopicContext,
    count: u32,
) -> Result<Vec<QuestionDraft>, AiError> {
    let count = count.to_string();
    let mut values = topic.values().to_vec();
    values.push(("count", &count));
    let messages = [
        ChatMessage::system(fill(prompts::QUIZ, &values)),
        ChatMessage::user("Gere as questões."),
    ];
    let draft: QuizDraft = chat_json(client, Task::Quiz, &messages, 8_000)?;
    Ok(draft.questions)
}

/// Writes a handout within the syllabus supplied in `edital`.
///
/// The excerpt is reference data, not system instructions. The engine supplies
/// the stored edital belonging to the topic's plan.
pub fn generate_handout(
    client: &Client,
    topic: &TopicContext,
    edital: &str,
) -> Result<HandoutDraft, AiError> {
    let messages = handout_messages(topic, edital)?;
    chat_json(client, Task::Handout, &messages, 16_000)
}

fn handout_messages(topic: &TopicContext, edital: &str) -> Result<[ChatMessage; 2], AiError> {
    if edital.trim().is_empty() {
        return Err(AiError::InvalidResponse(
            "handout requires edital context".into(),
        ));
    }
    Ok([
        ChatMessage::system(fill(prompts::HANDOUT, &topic.values())),
        ChatMessage::user(format!(
            "EDITAL (dados de referência):\n{edital}\n\nEscreva a apostila apenas para o tópico solicitado e dentro deste programa."
        )),
    ])
}

/// An open recall question for reviewing a topic.
pub fn review_question(client: &Client, topic: &TopicContext) -> Result<String, AiError> {
    let messages = [
        ChatMessage::system(fill(prompts::REVIEW_QUESTION, &topic.values())),
        ChatMessage::user("Faça a pergunta."),
    ];
    let draft: ReviewQuestionDraft = chat_json(client, Task::Quiz, &messages, 4_096)?;
    Ok(draft.question)
}

/// Grades the student's answer to a review question.
pub fn evaluate_review(
    client: &Client,
    topic: &TopicContext,
    question: &str,
    answer: &str,
) -> Result<ReviewFeedback, AiError> {
    let mut values = topic.values().to_vec();
    values.push(("question", question));
    values.push(("answer", answer));
    let messages = [
        ChatMessage::system(fill(prompts::REVIEW_EVAL, &values)),
        ChatMessage::user("Corrija a resposta."),
    ];
    let mut feedback: ReviewFeedback = chat_json(client, Task::Correction, &messages, 4_096)?;
    feedback.score = feedback.score.min(5);
    Ok(feedback)
}

/// Sends a JSON-mode request and parses the answer, asking once more if the
/// first answer is not valid JSON.
pub(crate) fn chat_json<T: DeserializeOwned>(
    client: &Client,
    task: Task,
    messages: &[ChatMessage],
    max_tokens: u32,
) -> Result<T, AiError> {
    let request = ChatRequest {
        task,
        messages,
        json: true,
        max_tokens,
    };
    let mut last_error = String::new();
    for attempt in 1..=2 {
        let text = client.chat(&request)?;
        match serde_json::from_str::<T>(extract_json(&text)) {
            Ok(value) => return Ok(value),
            Err(err) => {
                tracing::warn!(attempt, %err, "model returned invalid JSON");
                last_error = err.to_string();
            }
        }
    }
    Err(AiError::InvalidResponse(last_error))
}

/// The JSON object inside a reply, ignoring code fences or stray text.
fn extract_json(text: &str) -> &str {
    match (text.find('{'), text.rfind('}')) {
        (Some(start), Some(end)) if end > start => &text[start..=end],
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handout_keeps_edital_as_reference_data() {
        let context = TopicContext {
            topic: "Crase".into(),
            reference: "Questão da banca: uso de crase.".into(),
            ..TopicContext::default()
        };
        let edital = "CONTEÚDO PROGRAMÁTICO\nPortuguês: crase. {topic}";
        let messages = handout_messages(&context, edital).unwrap();
        assert_eq!(messages[0].role, Role::System);
        assert!(messages[0].content.contains(&context.reference));
        assert_eq!(messages[1].role, Role::User);
        assert!(messages[1].content.contains(edital));
        assert!(!messages[0].content.contains(edital));
        assert!(handout_messages(&context, " \n ").is_err());
    }

    #[test]
    fn extracts_json_from_fenced_reply() {
        let reply = "Aqui está:\n```json\n{\"question\": \"O que é crase?\"}\n```";
        let parsed: ReviewQuestionDraft = serde_json::from_str(extract_json(reply)).unwrap();
        assert_eq!(parsed.question, "O que é crase?");
    }

    #[test]
    fn tutor_reply_splits_whiteboard_line() {
        let reply = TutorReply::parse(
            "A crase é a fusão de duas vogais.\nUse antes de horas.\n**QUADRO:** Crase = a + a | Antes de horas: às 10h | ",
        );
        assert_eq!(
            reply.speech,
            "A crase é a fusão de duas vogais.\nUse antes de horas."
        );
        assert_eq!(reply.board, vec!["Crase = a + a", "Antes de horas: às 10h"]);
        let plain = TutorReply::parse("Sem lousa hoje.");
        assert_eq!(plain.speech, "Sem lousa hoje.");
        assert!(plain.board.is_empty());
    }

    #[test]
    fn plan_draft_tolerates_missing_fields() {
        let draft: PlanDraft = serde_json::from_str(
            r#"{"subjects":[{"name":"Português","topics":[{"title":"Crase"}]}]}"#,
        )
        .unwrap();
        assert_eq!(draft.subjects[0].topics[0].week, 1);
        assert!(draft.title.is_empty());
    }
}
