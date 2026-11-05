//! Mock exams: reading the exam's format from the notice and writing its
//! questions. The engine checks both before use.

use serde::Deserialize;

use crate::AiError;
use crate::client::{ChatMessage, Client};
use crate::config::Task;
use crate::prompts::{self, fill};
use crate::tasks::chat_json;

/// The exam's format as the model reads it from the notice.
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(default)]
pub struct FormatDraft {
    pub found: bool,
    pub minutes: u32,
    pub style: String,
    pub options: u32,
    pub wrong_cancels: bool,
    pub essay: bool,
    pub sections: Vec<FormatSectionDraft>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(default)]
pub struct FormatSectionDraft {
    pub subject: String,
    pub questions: u32,
    pub weight: f64,
}

/// Reads how the objective test is laid out, for `role`, using the plan's
/// subject names.
pub fn read_exam_format(
    client: &Client,
    edital: &str,
    role: &str,
    subjects: &[String],
) -> Result<FormatDraft, AiError> {
    let lang = client.language();
    let role = if role.trim().is_empty() {
        lang.pick(
            "o cargo principal do edital",
            "the main position of the notice",
        )
    } else {
        role.trim()
    };
    let list: Vec<String> = subjects.iter().map(|s| format!("- {s}")).collect();
    let system = fill(
        prompts::get(lang).exam_format,
        &[("role", role), ("subjects", &list.join("\n"))],
    );
    let messages = [
        ChatMessage::system(system),
        ChatMessage::user(format!("EDITAL:\n{edital}")),
    ];
    chat_json(client, Task::StudyPlan, &messages, 4_000)
}

/// What to ask for one subject of a mock exam.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExamQuestionsInput {
    pub exam: String,
    pub subject: String,
    /// Topic titles with how many questions each gets.
    pub topics: Vec<(String, u32)>,
    /// Past exam questions and board material (see [`crate::reference_block`]).
    pub reference: String,
    /// `None` for true/false statements; otherwise the number of options.
    pub options: Option<u32>,
}

impl ExamQuestionsInput {
    pub fn count(&self) -> u32 {
        self.topics.iter().map(|(_, n)| n).sum()
    }
}

/// A question as the model writes it; `topic` is 1-based.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExamQuestionDraft {
    #[serde(default)]
    pub topic: usize,
    pub prompt: String,
    #[serde(default)]
    pub options: Vec<String>,
    pub correct: usize,
    #[serde(default)]
    pub explanation: String,
}

#[derive(Deserialize)]
struct ExamQuestionsDraft {
    questions: Vec<ExamQuestionDraft>,
}

/// Writes the questions of one subject.
pub fn generate_exam_questions(
    client: &Client,
    input: &ExamQuestionsInput,
) -> Result<Vec<ExamQuestionDraft>, AiError> {
    let lang = client.language();
    let topics: Vec<String> = input
        .topics
        .iter()
        .enumerate()
        .filter(|(_, (_, n))| *n > 0)
        .map(|(i, (title, n))| {
            let label = lang.pick("questão(ões)", "question(s)");
            format!("{}. {title} — {n} {label}", i + 1)
        })
        .collect();
    let format = match input.options {
        None => lang.pick(
            "itens para julgar como Certo ou Errado. \"prompt\" é a afirmação, \"options\" fica vazio ([]) e \"correct\" é 0 se a afirmação está certa e 1 se está errada.",
            "statements to judge as true or false. \"prompt\" is the statement, \"options\" stays empty ([]) and \"correct\" is 0 if the statement is true and 1 if it is false.",
        )
        .to_owned(),
        Some(n) => match lang {
            crate::PromptLanguage::Portuguese => format!(
                "múltipla escolha com exatamente {n} alternativas e uma só correta. \"correct\" é o índice da alternativa correta, começando em 0."
            ),
            _ => format!(
                "multiple choice with exactly {n} options and only one right. \"correct\" is the index of the right option, starting at 0."
            ),
        },
    };
    let count = input.count().to_string();
    let system = fill(
        prompts::get(lang).exam_questions,
        &[
            ("exam", input.exam.as_str()),
            ("subject", input.subject.as_str()),
            ("topics", &topics.join("\n")),
            ("reference", input.reference.as_str()),
            ("format", &format),
            ("count", &count),
        ],
    );
    let messages = [
        client.cultured(system),
        ChatMessage::user(lang.pick("Gere as questões.", "Write the questions.")),
    ];
    // About 350 tokens a question, with room to spare.
    let budget = 1_000 + 450 * input.count();
    let draft: ExamQuestionsDraft = chat_json(client, Task::Quiz, &messages, budget.min(16_000))?;
    Ok(draft.questions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_draft_reads_partial_json() {
        let draft: FormatDraft = serde_json::from_str(
            r#"{"found": true, "sections": [{"subject": "Português", "questions": 10}]}"#,
        )
        .unwrap();
        assert!(draft.found);
        assert_eq!(draft.minutes, 0);
        assert_eq!(draft.sections[0].questions, 10);
        assert_eq!(draft.sections[0].weight, 0.0);
    }

    #[test]
    fn count_adds_the_topics() {
        let input = ExamQuestionsInput {
            topics: vec![("a".into(), 2), ("b".into(), 3)],
            ..Default::default()
        };
        assert_eq!(input.count(), 5);
    }
}
