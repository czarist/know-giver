//! Quizzes: multiple choice or true/false items (Cebraspe's Certo/Errado).

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::StudyError;
use crate::plan::TopicId;

/// The kind of questions a quiz asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuizStyle {
    /// Like the exam: true/false when its format is Certo/Errado.
    #[default]
    Exam,
    Choice,
    /// Statements to judge as right or wrong, as Cebraspe asks them.
    TrueFalse,
}

/// The words of a true/false question, true first: Portuguese (as Cebraspe
/// prints them) and English.
pub const TRUE_FALSE_WORDS: [[&str; 2]; 2] = [["Certo", "Errado"], ["True", "False"]];

/// One question: multiple choice, or a statement to judge (two options,
/// [`TRUE_FALSE_WORDS`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuizQuestion {
    pub prompt: String,
    pub options: Vec<String>,
    pub correct: usize,
    pub explanation: String,
    /// Taken from a real past exam rather than generated.
    pub from_board: bool,
}

impl QuizQuestion {
    /// A statement to judge as true or false rather than options to pick.
    pub fn is_true_false(&self) -> bool {
        is_true_false(&self.options)
    }

    /// Reorders the options with a seeded shuffle, keeping track of the right
    /// one. Models tend to put the correct option first; this undoes that.
    /// True/false questions keep "true" first, as on the answer sheet.
    pub fn shuffle_options(&mut self, seed: u64) {
        if self.is_true_false() {
            return;
        }
        let mut state = seed | 1;
        let mut next = move || {
            // xorshift64
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut order: Vec<usize> = (0..self.options.len()).collect();
        for i in (1..order.len()).rev() {
            let j = (next() % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
        let options = std::mem::take(&mut self.options);
        self.options = order.iter().map(|&i| options[i].clone()).collect();
        self.correct = order.iter().position(|&i| i == self.correct).unwrap_or(0);
    }
}

/// The options are a true/false pair, in any letter case.
pub fn is_true_false(options: &[String]) -> bool {
    TRUE_FALSE_WORDS.iter().any(|words| {
        options.len() == 2
            && options
                .iter()
                .zip(words)
                .all(|(option, word)| option.trim().eq_ignore_ascii_case(word))
    })
}

/// A validated quiz: at least one question, each with 2–6 options and a
/// valid correct answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quiz {
    topic_id: TopicId,
    questions: Vec<QuizQuestion>,
}

impl Quiz {
    pub fn new(topic_id: TopicId, questions: Vec<QuizQuestion>) -> Result<Self, StudyError> {
        if questions.is_empty() {
            return Err(StudyError::EmptyQuiz);
        }
        for (i, q) in questions.iter().enumerate() {
            let options_ok = (2..=6).contains(&q.options.len())
                && q.options.iter().all(|o| !o.trim().is_empty());
            if q.prompt.trim().is_empty() || !options_ok || q.correct >= q.options.len() {
                return Err(StudyError::InvalidQuestion(i + 1));
            }
        }
        Ok(Self {
            topic_id,
            questions,
        })
    }

    pub fn topic_id(&self) -> TopicId {
        self.topic_id
    }

    pub fn questions(&self) -> &[QuizQuestion] {
        &self.questions
    }
}

/// A finished quiz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuizAttempt {
    pub topic_id: TopicId,
    pub taken_on: NaiveDate,
    pub correct: u32,
    pub total: u32,
}

/// Aggregated quiz results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QuizStats {
    pub attempts: u32,
    pub correct: u32,
    pub total: u32,
}

impl QuizStats {
    /// Share of correct answers, or `None` with no answers yet.
    pub fn accuracy(&self) -> Option<f32> {
        (self.total > 0).then(|| self.correct as f32 / self.total as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(options: &[&str], correct: usize) -> QuizQuestion {
        QuizQuestion {
            prompt: "Qual?".into(),
            options: options.iter().map(|o| (*o).into()).collect(),
            correct,
            explanation: String::new(),
            from_board: false,
        }
    }

    #[test]
    fn rejects_out_of_range_answer() {
        let err = Quiz::new(TopicId(1), vec![question(&["a", "b"], 2)]).unwrap_err();
        assert!(matches!(err, StudyError::InvalidQuestion(1)));
    }

    #[test]
    fn rejects_empty_quiz() {
        assert!(matches!(
            Quiz::new(TopicId(1), vec![]),
            Err(StudyError::EmptyQuiz)
        ));
    }

    #[test]
    fn shuffle_keeps_the_correct_answer() {
        let mut positions = std::collections::HashSet::new();
        for seed in 0..40 {
            let mut q = question(&["certa", "b", "c", "d"], 0);
            q.shuffle_options(seed * 7919);
            assert_eq!(q.options[q.correct], "certa");
            assert_eq!(q.options.len(), 4);
            positions.insert(q.correct);
        }
        assert_eq!(positions.len(), 4, "correct answer should move around");
    }

    #[test]
    fn true_false_keeps_true_first() {
        for words in TRUE_FALSE_WORDS {
            let mut q = question(&words, 1);
            assert!(q.is_true_false());
            for seed in 0..20 {
                q.shuffle_options(seed);
                assert_eq!(q.options, words);
                assert_eq!(q.correct, 1);
            }
        }
        assert!(question(&["certo", " ERRADO "], 0).is_true_false());
        assert!(!question(&["Errado", "Certo"], 0).is_true_false());
        assert!(!question(&["a", "b"], 0).is_true_false());
    }

    #[test]
    fn accepts_valid_quiz() {
        assert!(Quiz::new(TopicId(1), vec![question(&["a", "b", "c"], 1)]).is_ok());
    }
}
