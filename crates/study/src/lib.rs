//! Study domain: plans, subjects, topics, reviews, quizzes and their storage.
//!
//! [`plan`], [`review`] and [`quiz`] are pure rules with no I/O.
//! [`store`] persists them in SQLite.

pub mod plan;
pub mod quiz;
pub mod review;
pub mod store;

pub use plan::{
    Difficulty, NewPlan, NewSubject, NewTopic, PlanId, PlanSummary, StudyPlan, Subject, SubjectId,
    Topic, TopicId, TopicStatus,
};
pub use quiz::{Quiz, QuizAttempt, QuizQuestion, QuizStats};
pub use review::{ReviewGrade, ReviewState};
pub use store::{Store, StoreError};

/// Errors raised by study rules.
///
/// Messages are English and meant for logs; the UI shows translated text.
#[derive(Debug, thiserror::Error)]
pub enum StudyError {
    #[error("study plan has no subjects")]
    EmptyPlan,
    #[error("subject '{0}' has no topics")]
    EmptySubject(String),
    #[error("quiz has no questions")]
    EmptyQuiz,
    #[error("quiz question {0} is malformed")]
    InvalidQuestion(usize),
}
