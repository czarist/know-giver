//! Mock exams (simulados): the exam's format, the questions, timed attempts
//! and their grade. Grading is plain arithmetic on the saved answer key;
//! the AI only writes questions.

use chrono::{Duration, NaiveDateTime};
use serde::{Deserialize, Serialize};

use crate::discursive::ProposalId;
use crate::plan::{PlanId, TopicId};

/// Most questions in one mock exam.
pub const MAX_QUESTIONS: u32 = 200;
/// Shortest and longest time allowed, in minutes.
pub const MIN_MINUTES: u32 = 10;
pub const MAX_MINUTES: u32 = 600;
/// Most sections (subjects) in one format.
pub const MAX_SECTIONS: usize = 30;
/// Highest weight of one question.
pub const MAX_WEIGHT: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MockExamId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MockAttemptId(pub i64);

/// How questions are answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnswerStyle {
    /// Pick one of `options` alternatives (A, B, C…).
    Choice { options: u8 },
    /// Judge a statement true or false (Certo/Errado).
    TrueFalse,
}

impl AnswerStyle {
    /// How many marks each question has on the answer card.
    pub fn marks(self) -> u8 {
        match self {
            Self::Choice { options } => options,
            Self::TrueFalse => 2,
        }
    }
}

/// Where the format came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatSource {
    /// Read from the notice by the AI and checked in code.
    Notice,
    /// Estimated by the app when the notice says nothing usable.
    Estimate,
    /// Set or changed by the student.
    Student,
}

/// One part of the objective test: a subject of the plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExamSection {
    pub subject: String,
    pub questions: u32,
    /// Points of each question.
    pub weight: f64,
}

/// The exam as the notice describes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExamFormat {
    pub minutes: u32,
    pub style: AnswerStyle,
    /// A wrong answer takes away one right answer's points (Cebraspe).
    pub wrong_cancels: bool,
    pub sections: Vec<ExamSection>,
    /// The exam has an essay or other written answer.
    pub essay: bool,
    pub source: FormatSource,
}

/// Why a format cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    #[error("the exam has no questions")]
    NoQuestions,
    #[error("the exam has more than {MAX_QUESTIONS} questions")]
    TooManyQuestions,
    #[error("the exam time is out of range")]
    Minutes,
    #[error("an answer style needs between 2 and 5 options")]
    Options,
    #[error("too many sections")]
    TooManySections,
    #[error("section {0} has no subject")]
    Subject(usize),
    #[error("section {0} has a bad weight")]
    Weight(usize),
}

impl ExamFormat {
    pub fn total_questions(&self) -> u32 {
        self.sections.iter().map(|s| s.questions).sum()
    }

    /// Most points the objective test gives.
    pub fn max_points(&self) -> f64 {
        self.sections
            .iter()
            .map(|s| f64::from(s.questions) * s.weight)
            .sum()
    }

    pub fn validate(&self) -> Result<(), FormatError> {
        if !(MIN_MINUTES..=MAX_MINUTES).contains(&self.minutes) {
            return Err(FormatError::Minutes);
        }
        if let AnswerStyle::Choice { options } = self.style
            && !(2..=5).contains(&options)
        {
            return Err(FormatError::Options);
        }
        if self.sections.len() > MAX_SECTIONS {
            return Err(FormatError::TooManySections);
        }
        for (i, section) in self.sections.iter().enumerate() {
            if section.subject.trim().is_empty() {
                return Err(FormatError::Subject(i + 1));
            }
            if !(section.weight.is_finite() && section.weight > 0.0 && section.weight <= MAX_WEIGHT)
            {
                return Err(FormatError::Weight(i + 1));
            }
        }
        match self.total_questions() {
            0 => Err(FormatError::NoQuestions),
            n if n > MAX_QUESTIONS => Err(FormatError::TooManyQuestions),
            _ => Ok(()),
        }
    }

    /// Sections with at least one question, in order.
    pub fn used_sections(&self) -> impl Iterator<Item = (usize, &ExamSection)> {
        self.sections
            .iter()
            .enumerate()
            .filter(|(_, s)| s.questions > 0)
    }
}

/// One question of a mock exam, numbered as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockQuestion {
    /// Index into the format's sections.
    pub section: usize,
    pub topic: Option<TopicId>,
    pub prompt: String,
    /// Empty for true/false questions.
    pub options: Vec<String>,
    /// Right option; for true/false, 0 is true and 1 is false.
    pub correct: u8,
    pub explanation: String,
}

/// A saved mock exam. Fixed once created.
#[derive(Debug, Clone, PartialEq)]
pub struct MockExam {
    pub id: MockExamId,
    pub plan_id: PlanId,
    pub title: String,
    pub format: ExamFormat,
    pub questions: Vec<MockQuestion>,
    /// The essay proposal, when the exam has one.
    pub proposal: Option<ProposalId>,
    pub model: String,
    pub created_at: NaiveDateTime,
}

/// A mock exam before it is saved.
#[derive(Debug, Clone, PartialEq)]
pub struct NewMockExam {
    pub title: String,
    pub format: ExamFormat,
    pub questions: Vec<MockQuestion>,
    pub proposal: Option<ProposalId>,
    pub model: String,
}

/// Why a set of questions cannot be saved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MockError {
    #[error("bad format: {0}")]
    Format(#[from] FormatError),
    #[error("expected {expected} questions, got {got}")]
    Count { expected: usize, got: usize },
    #[error("question {0} is not valid")]
    Question(usize),
}

impl NewMockExam {
    /// Checks the questions against the format: right count per section
    /// in order, the right number of options and a valid answer.
    pub fn validate(&self) -> Result<(), MockError> {
        self.format.validate()?;
        let expected = self.format.total_questions() as usize;
        if self.questions.len() != expected {
            return Err(MockError::Count {
                expected,
                got: self.questions.len(),
            });
        }
        let mut sections = self
            .format
            .used_sections()
            .flat_map(|(i, s)| std::iter::repeat_n(i, s.questions as usize));
        for (n, q) in self.questions.iter().enumerate() {
            let options_ok = match self.format.style {
                AnswerStyle::Choice { options } => {
                    q.options.len() == options as usize
                        && q.options.iter().all(|o| !o.trim().is_empty())
                }
                AnswerStyle::TrueFalse => q.options.is_empty(),
            };
            if sections.next() != Some(q.section)
                || q.prompt.trim().is_empty()
                || !options_ok
                || q.correct >= self.format.style.marks()
            {
                return Err(MockError::Question(n + 1));
            }
        }
        Ok(())
    }
}

/// One sitting of a mock exam. The clock runs from `started_at`; answers
/// are saved as they are marked.
#[derive(Debug, Clone, PartialEq)]
pub struct MockAttempt {
    pub id: MockAttemptId,
    pub exam_id: MockExamId,
    pub started_at: NaiveDateTime,
    pub finished_at: Option<NaiveDateTime>,
    /// One per question; `None` is left blank.
    pub answers: Vec<Option<u8>>,
}

impl MockAttempt {
    pub fn deadline(&self, format: &ExamFormat) -> NaiveDateTime {
        self.started_at + Duration::minutes(i64::from(format.minutes))
    }

    /// Time left at `now`; negative once the time is up.
    pub fn remaining(&self, format: &ExamFormat, now: NaiveDateTime) -> Duration {
        self.deadline(format) - now
    }

    /// How long the student took, once handed in.
    pub fn time_used(&self) -> Option<Duration> {
        self.finished_at.map(|end| end - self.started_at)
    }

    /// Handed in after the time was up.
    pub fn overtime(&self, format: &ExamFormat) -> bool {
        self.finished_at
            .is_some_and(|end| end > self.deadline(format))
    }
}

/// The grade of one section.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionResult {
    pub section: usize,
    pub right: u32,
    pub wrong: u32,
    pub blank: u32,
    pub points: f64,
    pub max: f64,
}

/// The grade of an attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct MockResult {
    pub sections: Vec<SectionResult>,
    pub right: u32,
    pub wrong: u32,
    pub blank: u32,
    /// Can be below zero when wrong answers cancel right ones.
    pub points: f64,
    pub max: f64,
}

impl MockResult {
    /// Points as a share of the maximum, in `0.0..=1.0` (negative scores
    /// count as zero).
    pub fn share(&self) -> f64 {
        if self.max > 0.0 {
            (self.points / self.max).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// Grades `answers` against the exam's key.
pub fn grade(exam: &MockExam, answers: &[Option<u8>]) -> MockResult {
    let format = &exam.format;
    let mut sections: Vec<SectionResult> = format
        .used_sections()
        .map(|(i, s)| SectionResult {
            section: i,
            right: 0,
            wrong: 0,
            blank: 0,
            points: 0.0,
            max: f64::from(s.questions) * s.weight,
        })
        .collect();
    for (n, question) in exam.questions.iter().enumerate() {
        let Some(result) = sections.iter_mut().find(|r| r.section == question.section) else {
            continue;
        };
        let weight = format.sections[question.section].weight;
        match answers.get(n).copied().flatten() {
            None => result.blank += 1,
            Some(answer) if answer == question.correct => {
                result.right += 1;
                result.points += weight;
            }
            Some(_) => {
                result.wrong += 1;
                if format.wrong_cancels {
                    result.points -= weight;
                }
            }
        }
    }
    MockResult {
        right: sections.iter().map(|s| s.right).sum(),
        wrong: sections.iter().map(|s| s.wrong).sum(),
        blank: sections.iter().map(|s| s.blank).sum(),
        points: sections.iter().map(|s| s.points).sum(),
        max: format.max_points(),
        sections,
    }
}

/// Splits `count` questions over `topics` as evenly as possible, earlier
/// topics first. Returns one count per topic.
pub fn spread(count: u32, topics: usize) -> Vec<u32> {
    if topics == 0 {
        return Vec::new();
    }
    let base = count / topics as u32;
    let extra = (count % topics as u32) as usize;
    (0..topics).map(|i| base + u32::from(i < extra)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(style: AnswerStyle, wrong_cancels: bool) -> ExamFormat {
        ExamFormat {
            minutes: 120,
            style,
            wrong_cancels,
            sections: vec![
                ExamSection {
                    subject: "Português".into(),
                    questions: 2,
                    weight: 1.0,
                },
                ExamSection {
                    subject: "Direito".into(),
                    questions: 0,
                    weight: 1.0,
                },
                ExamSection {
                    subject: "Informática".into(),
                    questions: 1,
                    weight: 2.0,
                },
            ],
            essay: false,
            source: FormatSource::Student,
        }
    }

    fn question(section: usize, style: AnswerStyle, correct: u8) -> MockQuestion {
        MockQuestion {
            section,
            topic: None,
            prompt: "Pergunta".into(),
            options: match style {
                AnswerStyle::Choice { options } => {
                    (0..options).map(|i| format!("opção {i}")).collect()
                }
                AnswerStyle::TrueFalse => Vec::new(),
            },
            correct,
            explanation: String::new(),
        }
    }

    fn exam(style: AnswerStyle, wrong_cancels: bool) -> MockExam {
        MockExam {
            id: MockExamId(1),
            plan_id: PlanId(1),
            title: "Simulado".into(),
            format: format(style, wrong_cancels),
            questions: vec![
                question(0, style, 0),
                question(0, style, 1),
                question(2, style, 1),
            ],
            proposal: None,
            model: String::new(),
            created_at: NaiveDateTime::default(),
        }
    }

    #[test]
    fn format_rejects_out_of_range_values() {
        let mut f = format(AnswerStyle::Choice { options: 5 }, false);
        assert_eq!(f.validate(), Ok(()));
        f.minutes = 5;
        assert_eq!(f.validate(), Err(FormatError::Minutes));
        f.minutes = 60;
        f.style = AnswerStyle::Choice { options: 7 };
        assert_eq!(f.validate(), Err(FormatError::Options));
        f.style = AnswerStyle::TrueFalse;
        f.sections[0].weight = 0.0;
        assert_eq!(f.validate(), Err(FormatError::Weight(1)));
        f.sections[0].weight = 1.0;
        f.sections[0].questions = MAX_QUESTIONS;
        assert_eq!(f.validate(), Err(FormatError::TooManyQuestions));
        for s in &mut f.sections {
            s.questions = 0;
        }
        assert_eq!(f.validate(), Err(FormatError::NoQuestions));
    }

    #[test]
    fn new_exam_must_follow_the_format_in_order() {
        let style = AnswerStyle::Choice { options: 4 };
        let e = exam(style, false);
        let mut new = NewMockExam {
            title: e.title.clone(),
            format: e.format.clone(),
            questions: e.questions.clone(),
            proposal: None,
            model: String::new(),
        };
        assert_eq!(new.validate(), Ok(()));
        new.questions.swap(1, 2);
        assert_eq!(new.validate(), Err(MockError::Question(2)));
        new.questions.swap(1, 2);
        new.questions[0].options.pop();
        assert_eq!(new.validate(), Err(MockError::Question(1)));
        new.questions.pop();
        assert!(matches!(new.validate(), Err(MockError::Count { .. })));
    }

    #[test]
    fn true_false_questions_have_no_options() {
        let e = exam(AnswerStyle::TrueFalse, true);
        let mut new = NewMockExam {
            title: e.title.clone(),
            format: e.format.clone(),
            questions: e.questions.clone(),
            proposal: None,
            model: String::new(),
        };
        assert_eq!(new.validate(), Ok(()));
        new.questions[2].correct = 2;
        assert_eq!(new.validate(), Err(MockError::Question(3)));
    }

    #[test]
    fn grade_counts_right_wrong_and_blank_per_section() {
        let e = exam(AnswerStyle::Choice { options: 4 }, false);
        let result = grade(&e, &[Some(0), None, Some(1)]);
        assert_eq!((result.right, result.wrong, result.blank), (2, 0, 1));
        assert_eq!(result.points, 3.0);
        assert_eq!(result.max, 4.0);
        assert_eq!(result.sections.len(), 2);
        assert_eq!(result.sections[1].section, 2);
        assert_eq!(result.sections[1].points, 2.0);
    }

    #[test]
    fn wrong_answers_cancel_right_ones_when_the_notice_says_so() {
        let e = exam(AnswerStyle::TrueFalse, true);
        let result = grade(&e, &[Some(1), Some(0), Some(0)]);
        assert_eq!((result.right, result.wrong), (0, 3));
        assert_eq!(result.points, -4.0);
        assert_eq!(result.share(), 0.0);
    }

    #[test]
    fn missing_answers_count_as_blank() {
        let e = exam(AnswerStyle::Choice { options: 4 }, false);
        let result = grade(&e, &[]);
        assert_eq!(result.blank, 3);
        assert_eq!(result.points, 0.0);
    }

    #[test]
    fn attempt_knows_its_deadline_and_overtime() {
        let f = format(AnswerStyle::TrueFalse, false);
        let start = NaiveDateTime::default();
        let mut attempt = MockAttempt {
            id: MockAttemptId(1),
            exam_id: MockExamId(1),
            started_at: start,
            finished_at: None,
            answers: Vec::new(),
        };
        assert_eq!(attempt.remaining(&f, start), Duration::minutes(120));
        assert!(!attempt.overtime(&f));
        attempt.finished_at = Some(start + Duration::minutes(121));
        assert!(attempt.overtime(&f));
        assert_eq!(attempt.time_used(), Some(Duration::minutes(121)));
    }

    #[test]
    fn spread_shares_questions_evenly() {
        assert_eq!(spread(10, 3), vec![4, 3, 3]);
        assert_eq!(spread(2, 4), vec![1, 1, 0, 0]);
        assert!(spread(5, 0).is_empty());
    }
}
