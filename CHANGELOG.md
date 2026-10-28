# Changelog

All notable changes to Know Giver. Format based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/).

## [0.9.0] - 2026-10-27

### Added
- Study handouts (apostilas) as PDF: the AI writes each topic as an introduction,
  sections with key points, examples, exam pitfalls, a summary and five exercises with a
  commented answer key.
- New `kg-handout` crate: renders handouts with Typst as a library (pure Rust, fonts
  embedded), with cover, table of contents, numbered sections and page headers. Content
  goes to the template as JSON, so model output is never read as markup.
- Plan screen: handout per topic, per subject or for the whole plan, with progress and
  cancel; right-click writes a topic again. The PDF opens when it is ready.
- Written topics are cached in SQLite (migration 2), so exporting again costs no AI calls.
  Up to three topics are written at the same time.
- Optional handout model (`MODEL_HANDOUT`, or Settings); empty means the plan model.
- Exercise options are shuffled, like quiz options.

## [0.8.0] - 2026-10-27

### Added
- Speech text normalization for Brazilian Portuguese before synthesis: legal
  abbreviations ("art. 5º, § 2º, da CF/88" → "artigo quinto, parágrafo segundo, da
  Constituição Federal de 1988"), law numbers and years ("Lei nº 8.112/90" → "Lei número
  8112 de 1990"), ordinals, dates, roman numerals after "inciso"/"título", percentages,
  money, brackets and dashes.
- Softer playback: gentle low-pass against harsh sibilants, equal loudness for every
  chunk, fades against clicks and a short pause between chunks.

### Changed
- Piper pauses between sentences (`--sentence_silence 0.25`).
- Default speech speed slightly slower (0.95).
- Tutor prompt asks for spoken style: short sentences, natural connectors, no
  parentheses, abbreviations written out.
- `voice_check` example speaks a legal sentence and can save the result as WAV.

## [0.7.1] - 2026-10-27

### Added
- Roadmap phase 5: optional upload of past exams and board material, with a relevance
  filter against the edital, and written study handouts exported as PDF.
- Roadmap phase 6: meeting-style lessons with an animated teacher, visual identity (logo,
  palette, fonts, icons) and UX (Today screen, onboarding, empty states).
- Roadmap phase 7: more natural speech, female and male voices (Kokoro), voice samples.

### Changed
- README: features, running, packaging and development checks.
- Architecture document: decisions on speech, AI, edital excerpt, background work and
  translations.
- Agent instructions: actual speech stack (Whisper on candle, Piper as a child process)
  and the rule that every UI string comes from the translation files.

## [0.7.0] - 2026-10-27

### Added
- App icon drawn in code and embedded into `know-giver.exe`
  (`cargo run -p kg-ui --example write_icon` regenerates the `.ico`).
- `scripts/package.ps1`: release build and portable zip
  (`dist/KnowGiver-<version>-windows-x64.zip`).
- Inno Setup script for a per-user Windows installer in Portuguese and English, built by
  the package script when Inno Setup 6 is installed.
- `LEIA-ME.txt` with first steps for end users.

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
