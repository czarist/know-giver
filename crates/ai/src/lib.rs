//! AI layer: OpenAI-compatible client and tutor tasks.
//!
//! Works with any OpenAI-compatible endpoint (Z.ai/GLM, Ollama, LiteLLM).
//! Each task uses its own model (see [`ModelSet`]) and falls back to
//! `models.fallback` when the main model is rate limited or unavailable.
//! Prompts live in `crates/ai/prompts/` and follow `docs/tutor-prompts.md`.

mod board;
mod client;
mod config;
mod culture;
mod discursive;
mod handout_review;
mod prompts;
mod providers;
mod tasks;
mod throttle;

pub use board::{
    AnswerDraft, BoardContext, BoardQuestionDraft, ExcerptDraft, RelevanceDraft, chunk_text,
    classify_board_items, extract_answer_key, extract_board_excerpts, extract_board_questions,
    reference_block,
};
pub use client::{ChatMessage, Client, Role};
pub use config::{AiConfig, ModelSet, PromptLanguage, Task};
pub use culture::ExamCulture;
pub use discursive::{
    CriterionDraft, CriterionInput, EvaluationDraft, EvaluationInput, EvidenceDraft, IssueDraft,
    IssueExampleDraft, IssueInput, PageReading, ProblemDraft, ProposalDraft, ProposalInput,
    RemarkDraft, RubricDraft, ScoreDraft, UncertainDraft, check_vision, evaluate_writing,
    find_recurring_issues, generate_proposal, numbered_text, prompt_version, read_manuscript_page,
};
pub use handout_review::{ReviewedHandout, review_handout};
pub use prompts::handout_pipeline as handout_pipeline_prompts;
pub use prompts::{Prompts, get as prompts_in};
pub use providers::{
    FreeData, FreeTier, Market, ModelChoice, PROVIDERS, Provider, ProviderKind, Recommendation,
    provider,
    provider_for_url,
};
pub use tasks::{
    HandoutDraft, LessonLength, PlanDraft, PlanPrefs, QuestionDraft, ReviewFeedback, SectionDraft,
    SubjectDraft, TopicContext, TopicDraft, TutorReply, evaluate_review, find_roles,
    generate_handout, generate_plan, generate_quiz, review_question, review_question_avoiding,
    start_lesson, start_lesson_stream, tutor_reply, tutor_reply_stream,
};
pub use throttle::Throttle;

/// Errors from talking to the model.
///
/// Messages are English and meant for logs; the UI shows translated text.
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("could not connect to {url}: {reason}")]
    Connection { url: String, reason: String },
    #[error("authentication rejected by the AI server (HTTP {status})")]
    Auth { status: u16 },
    #[error("rate limited by the AI server")]
    RateLimited,
    #[error("AI server returned HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("invalid model response: {0}")]
    InvalidResponse(String),
    #[error("the model used its whole answer budget (likely reasoning) and wrote nothing")]
    OutOfTokens,
    #[error("no vision model is configured")]
    NoVisionModel,
    #[error("the vision request was rejected (HTTP {status}); the model may not accept images")]
    VisionRejected { status: u16 },
}
