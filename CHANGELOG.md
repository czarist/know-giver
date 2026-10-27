# Changelog

All notable changes to Know Giver. Format based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/).

## [0.3.0] - 2026-10-27

### Added
- OpenAI-compatible chat client (Z.ai/GLM, Ollama, LiteLLM) with a model per task and a
  fallback model on rate limits and server errors.
- Configuration from `.env`: `OPENAI_API_BASE`, `OPENAI_API_KEY`, `MODEL_*`.
- Tutor prompts in `crates/ai/prompts/` (pt-BR): study plan, lesson, quiz, review
  question, review grading.
- JSON tasks with one retry on invalid JSON.
- `live_check` example that exercises every task against the real endpoint.

### Changed
- `reasoning_effort: "low"` on every request. Without it, GLM 5.x spent the whole token
  budget reasoning on a full edital and returned no plan.

## [0.2.0] - 2026-10-27

### Added
- Edital text extraction from PDF (pdf-extract), safe against extractor panics.
- Whitespace normalization and a syllabus-aware excerpt for the prompt: the opening of
  the edital plus its "conteúdo programático" (TJ-CE edital: 111k → 25k characters).

## [0.1.0] - 2026-10-27

### Added
- Cargo workspace (`crates/*`) with shared dependencies and lints: no `unwrap`, `expect`
  or `panic!` outside tests.
- `kg-study`: study plans (subjects, topics, difficulty, status, week), SM-2 spaced
  repetition, multiple-choice quizzes, and SQLite storage with migrations.
- `.gitignore` and `.env.example`.
