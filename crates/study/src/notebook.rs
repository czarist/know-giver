//! What the student keeps from studying: quiz questions they got wrong and
//! what each lesson covered.

use chrono::NaiveDate;

use crate::plan::TopicId;
use crate::quiz::QuizQuestion;

/// Row id of a mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MistakeId(pub i64);

/// A quiz question the student got wrong and has not got right since.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mistake {
    pub id: MistakeId,
    pub topic_id: TopicId,
    pub question: QuizQuestion,
    /// The option the student picked the last time.
    pub chosen: usize,
    pub missed_on: NaiveDate,
    /// How many times this question was missed.
    pub misses: u32,
}

impl Mistake {
    /// One line for the tutor: the question, the right answer and the
    /// student's wrong one.
    pub fn summary(&self) -> String {
        let q = &self.question;
        let option = |i: usize| q.options.get(i).map_or("", String::as_str);
        format!(
            "{} — certa: {} — o aluno marcou: {}",
            q.prompt.split_whitespace().collect::<Vec<_>>().join(" "),
            option(q.correct),
            option(self.chosen)
        )
    }
}

/// What a finished lesson covered, kept to look back at.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LessonRecap {
    pub taken_on: Option<NaiveDate>,
    /// The lesson plan's steps.
    pub outline: Vec<String>,
    /// What the tutor wrote on the board.
    pub board: Vec<String>,
    /// The tutor's closing words.
    pub summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_names_both_answers() {
        let mistake = Mistake {
            id: MistakeId(1),
            topic_id: TopicId(1),
            question: QuizQuestion {
                prompt: "Quem escreveu  O Contrato Social?".into(),
                options: vec!["Hobbes".into(), "Rousseau".into()],
                correct: 1,
                explanation: String::new(),
                from_board: false,
            },
            chosen: 0,
            missed_on: NaiveDate::from_ymd_opt(2026, 10, 29).unwrap(),
            misses: 1,
        };
        assert_eq!(
            mistake.summary(),
            "Quem escreveu O Contrato Social? — certa: Rousseau — o aluno marcou: Hobbes"
        );
    }
}
