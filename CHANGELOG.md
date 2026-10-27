# Changelog

All notable changes to Know Giver. Format based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/).

## [0.6.0] - 2026-10-27

### Added
- Desktop interface (egui) with seven screens:
  - **Edital**: pick the PDF, see what was extracted, set weeks, hours per week and role,
    generate the plan; list, open and delete saved plans.
  - **Study plan**: by week (current week first) or by subject, with status, difficulty,
    reviews due and shortcuts to lesson and quiz.
  - **Lesson**: transcript, voice orb that shows listening, thinking and speaking, mic
    level, typed fallback, warnings when speech models are missing.
  - **Quiz**: one question at a time, right/wrong feedback with explanation, score.
  - **Review**: topics due today and in the next days; AI recall question, graded answer,
    or self-grading.
  - **Progress**: totals, per-subject bars, Markdown report export.
  - **Settings**: language, AI server, key and per-task models, connection test, voice
    options, local model downloads with progress.
- Translations in `locales/pt-BR.yml` and `locales/en.yml`; no hardcoded UI text. Tests
  check that both languages have the same keys and that every key used in code exists.
- Calm light and dark theme, notices as toasts, badges in the sidebar.
- `know-giver` binary: reads `.env` from the working folder or next to the executable,
  no console window in release builds.
- Snapshot test that renders every screen to PNG (needs a GPU, ignored by default).

### Fixed
- A global `OPENAI_API_KEY` in the system no longer overrides the project's `.env`.

## [0.5.0] - 2026-10-27

### Added
- Engine that owns all app state: the UI sends commands, slow work runs on worker
  threads, and results come back as events. Results from closed sessions are dropped.
- Edital import and plan generation; several plans, with the active one remembered.
- Voice lesson loop: listen → transcribe → tutor → speak sentence by sentence, with
  interrupt and an optional hands-free mode that listens again after the tutor speaks.
- Quizzes recorded and turned into a spaced-repetition grade.
- Reviews: AI recall question and AI-graded answer, or self-grade; SM-2 scheduling.
- Settings stored in SQLite; an API key that comes from `.env` is not copied there.
- Model downloads with progress, cancel and remove.
- `e2e` example: real edital → plan → quiz → review against the configured AI server.

### Fixed
- Quiz options are shuffled after generation; the model tended to put the right answer
  first.

## [0.4.0] - 2026-10-27

### Added
- Microphone capture (cpal) with noise-floor end-of-speech detection and idle timeout.
- Speech-to-text with Whisper on candle, pure Rust on CPU: slaney mel filterbank,
  byte-level token decoder, greedy decoding with a repetition guard, silence skip.
- Text-to-speech with the Piper executable as a child process, text split into sentence
  chunks so speech starts early.
- Playback on its own thread (rodio).
- Local model store: Whisper tiny/base/small, Piper, pt-BR voices Faber, Cadu and Jeff;
  downloads through `.part` files so interrupted downloads never look installed.
- `stt_check` and `voice_check` examples.

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
