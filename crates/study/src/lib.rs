//! Study domain: plans, subjects, topics, reviews, quizzes and their storage.
//!
//! [`plan`], [`review`] and [`quiz`] are pure rules with no I/O.
//! [`store`] persists them in SQLite.

pub mod activity;
pub mod board;
pub mod discursive;
pub mod notebook;
pub mod plan;
pub mod quiz;
pub mod reading;
pub mod review;
pub mod store;

pub use activity::{
    ActivityKind, PROGRESS_MILESTONES, STREAK_MILESTONES, progress_milestone, streak,
    streak_milestone,
};
pub use board::{
    BoardDocument, BoardExcerpt, BoardQuestion, BoardQuestionId, DocumentId, DocumentKind,
    ExcerptId, NewBoardExcerpt, NewBoardQuestion, Relevance, answer_index,
};
pub use discursive::{
    Attempt, AttemptId, AttemptOrigin, AttemptStatus, ConfirmedTranscript, Criterion,
    CriterionScore, DifficultyReport, Edit, Evaluation, Evidence, IssueExample, Legibility,
    MAX_PAGES, NewPage, NewProposal, Page, PageId, PageIssue, Problem, ProblemCategory, Proposal,
    ProposalId, RecurringIssue, Remark, Rubric, RubricError, RubricSource, Stage, Transcript,
    TranscriptPage, TranscriptRecord, Uncertain, Unreadable, WritingKind, WritingRules,
    writing_rules,
};
pub use notebook::{LessonRecap, Mistake, MistakeId};
pub use plan::{
    Difficulty, NewPlan, NewSubject, NewTopic, PlanId, PlanSummary, StudyPlan, Subject, SubjectId,
    Topic, TopicId, TopicStatus,
};
pub use quiz::{Quiz, QuizAttempt, QuizQuestion, QuizStats};
pub use reading::{Highlight, HighlightColor, HighlightId, Reading, ReadingId};
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
