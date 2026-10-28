//! AI layer: OpenAI-compatible client and tutor tasks.
//!
//! Works with any OpenAI-compatible endpoint (Z.ai/GLM, Ollama, LiteLLM).
//! Each task uses its own model (see [`ModelSet`]) and falls back to
//! `models.fallback` when the main model is rate limited or unavailable.
//! Prompts live in `crates/ai/prompts/` and follow `docs/tutor-prompts.md`.

mod board;
mod client;
mod config;
mod prompts;
mod tasks;

pub use board::{
    AnswerDraft, BoardContext, BoardQuestionDraft, ExcerptDraft, RelevanceDraft, chunk_text,
    classify_board_items, extract_answer_key, extract_board_excerpts, extract_board_questions,
    reference_block,
};
pub use client::{ChatMessage, Client, Role};
pub use config::{AiConfig, ModelSet, Task};
pub use prompts::HANDOUT as HANDOUT_PROMPT;
pub use tasks::{
    HandoutDraft, PlanDraft, PlanPrefs, QuestionDraft, ReviewFeedback, SectionDraft, SubjectDraft,
    TopicContext, TopicDraft, TutorReply, evaluate_review, generate_handout, generate_plan,
    generate_quiz, review_question, start_lesson, start_lesson_stream, tutor_reply,
    tutor_reply_stream,
};

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
}
