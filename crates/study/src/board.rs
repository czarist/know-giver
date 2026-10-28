//! Exam board material: past exams, answer keys and handouts uploaded by the
//! student, split into questions and excerpts, each checked against the plan.

use chrono::NaiveDate;

use crate::plan::{PlanId, TopicId};

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub i64);
    };
}

id_type!(DocumentId);
id_type!(BoardQuestionId);
id_type!(ExcerptId);

/// What an uploaded file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocumentKind {
    /// A past exam (prova anterior).
    Exam,
    /// Answers for an exam (gabarito).
    AnswerKey,
    /// Any other board material (apostila, guide).
    Material,
}

impl DocumentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exam => "exam",
            Self::AnswerKey => "answer_key",
            Self::Material => "material",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Exam, Self::AnswerKey, Self::Material]
            .into_iter()
            .find(|k| k.as_str() == value)
    }
}

/// How well an item matches the edital's syllabus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Relevance {
    Relevant,
    Partial,
    /// Other role, subject not in this edital, removed topic. Kept but unused.
    OutOfScope,
}

impl Relevance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Relevant => "relevant",
            Self::Partial => "partial",
            Self::OutOfScope => "out",
        }
    }

    /// Lenient parser for model output; unknown values count as out of scope.
    pub fn parse_lenient(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "relevant" | "relevante" => Self::Relevant,
            "partial" | "parcial" | "partly" => Self::Partial,
            _ => Self::OutOfScope,
        }
    }

    pub fn is_used(self) -> bool {
        self != Self::OutOfScope
    }
}

/// An uploaded file, with counts of what was taken from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardDocument {
    pub id: DocumentId,
    pub plan_id: PlanId,
    pub kind: DocumentKind,
    pub file_name: String,
    pub added_on: NaiveDate,
    pub used: usize,
    pub set_aside: usize,
    /// Exam questions still without a known answer.
    pub unanswered: usize,
}

/// A question taken from a past exam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardQuestion {
    pub id: BoardQuestionId,
    pub document_id: DocumentId,
    /// Number printed on the exam, used to match the answer key.
    pub number: Option<u32>,
    pub prompt: String,
    pub options: Vec<String>,
    pub correct: Option<usize>,
    pub topic_id: Option<TopicId>,
    pub relevance: Relevance,
    /// Why it was classified this way (from the model, or "manual").
    pub reason: String,
}

/// A summarized passage of board material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardExcerpt {
    pub id: ExcerptId,
    pub document_id: DocumentId,
    pub summary: String,
    pub topic_id: Option<TopicId>,
    pub relevance: Relevance,
    pub reason: String,
}

/// A question before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBoardQuestion {
    pub number: Option<u32>,
    pub prompt: String,
    pub options: Vec<String>,
    pub correct: Option<usize>,
    pub topic_id: Option<TopicId>,
    pub relevance: Relevance,
    pub reason: String,
}

impl NewBoardQuestion {
    /// Questions without a topic cannot be used, whatever the model said.
    pub fn normalized(mut self) -> Self {
        if self.topic_id.is_none() {
            self.relevance = Relevance::OutOfScope;
        }
        if self.correct.is_some_and(|c| c >= self.options.len()) {
            self.correct = None;
        }
        self
    }

    pub fn is_valid(&self) -> bool {
        !self.prompt.trim().is_empty() && (2..=6).contains(&self.options.len())
    }
}

/// An excerpt before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBoardExcerpt {
    pub summary: String,
    pub topic_id: Option<TopicId>,
    pub relevance: Relevance,
    pub reason: String,
}

impl NewBoardExcerpt {
    pub fn normalized(mut self) -> Self {
        if self.topic_id.is_none() {
            self.relevance = Relevance::OutOfScope;
        }
        self
    }
}

/// Converts an answer letter ("A".."F", or "C"/"E" for Certo/Errado items)
/// to an option index, given the options of the question.
pub fn answer_index(answer: &str, options: &[String]) -> Option<usize> {
    let answer = answer.trim().to_uppercase();
    let is_true_false = options.len() == 2
        && options[0].trim().eq_ignore_ascii_case("certo")
        && options[1].trim().eq_ignore_ascii_case("errado");
    if is_true_false {
        return match answer.as_str() {
            "C" | "CERTO" => Some(0),
            "E" | "ERRADO" => Some(1),
            _ => None,
        };
    }
    let letter = answer.chars().next()?;
    let index = (letter as usize).checked_sub('A' as usize)?;
    (answer.len() == 1 && index < options.len()).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).into()).collect()
    }

    #[test]
    fn answer_letters() {
        let abcd = opts(&["a", "b", "c", "d"]);
        assert_eq!(answer_index("b", &abcd), Some(1));
        assert_eq!(answer_index("E", &abcd), None);
        assert_eq!(answer_index("anulada", &abcd), None);
        let tf = opts(&["Certo", "Errado"]);
        assert_eq!(answer_index("C", &tf), Some(0));
        assert_eq!(answer_index("E", &tf), Some(1));
    }

    #[test]
    fn question_without_topic_is_out_of_scope() {
        let q = NewBoardQuestion {
            number: Some(1),
            prompt: "Qual?".into(),
            options: opts(&["a", "b"]),
            correct: Some(5),
            topic_id: None,
            relevance: Relevance::Relevant,
            reason: String::new(),
        }
        .normalized();
        assert_eq!(q.relevance, Relevance::OutOfScope);
        assert_eq!(q.correct, None);
    }

    #[test]
    fn relevance_parsing() {
        assert_eq!(Relevance::parse_lenient("Relevante"), Relevance::Relevant);
        assert_eq!(Relevance::parse_lenient("partial"), Relevance::Partial);
        assert_eq!(Relevance::parse_lenient("???"), Relevance::OutOfScope);
    }
}
