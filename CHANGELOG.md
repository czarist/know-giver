# Changelog

All notable changes to Know Giver. Format based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/).

## [0.19.1] - 2026-10-28

### Fixed
- "Prepare everything" no longer loses topics to the AI server's rate limit. Requests
  share one pacer: when the server answers "too many requests", every worker pauses
  together (honouring `Retry-After`), fewer requests run at once, and the pace grows
  back after a run of successes. The rejected request is retried on its own, so a
  handout is not rewritten from scratch. Topics that still fail get one more pass, one
  at a time, at the end. The plan screen says when the server asked for a pause.

## [0.19.0] - 2026-10-28

### Added
- Prepare a whole plan at once. When creating a plan, "Prepare handouts and quizzes for
  every topic now" (on by default) writes, in the background, each topic's reviewed
  handout and a bank of 10 quiz questions, then saves the full plan PDF to
  Documents › Know Giver. Existing plans get "Prepare everything now" on the Plan
  screen; it only writes what is missing, so it also resumes after a restart or retries
  failed topics. Busy or unreachable servers are retried twice before a topic is
  counted as failed.
- Quizzes draw from the prepared question bank first (after real past-exam questions),
  so they start instantly; the model only writes what is still missing.
- A "working on it" panel for long jobs: the teacher thinking, what is being written
  right now, progress with a time estimate, "this can take a while", and a study tip
  that changes every few seconds. Shown while the plan is generated and while content
  is prepared; the Plan item in the sidebar shows the topic count meanwhile.

### Changed
- Plan PDFs and single handouts share the same writing and rendering code; database
  migration adds the `quiz_banks` table.

## [0.18.0] - 2026-10-28

### Fixed
- Kokoro voices failed every synthesis on Windows: the model folder was passed as a
  `\\?\` path, and sherpa-onnx appends `/phontab` with a forward slash, which such
  paths reject. The folder is now passed as a plain absolute path.
- GLM 4.x models ignored `reasoning_effort: "low"` and reasoned at length before
  answering; a handout on `glm-4.7-flash` took 264 s just to write. Fast responses now
  send `thinking: {"type": "disabled"}` to GLM 4.x (GLM 5.x rejects it and keeps
  `reasoning_effort`).
- Short spoken answers were misheard: Whisper turned a lone "sim" into video-ending
  phrases such as "Tchau!" or "Até a próxima!". Whisper is now given the tutor's last
  words as context, and a transcript that is only such a phrase is dropped so the
  student is asked to repeat instead of the tutor answering "Tchau!".

### Changed
- Handout review returns only its corrections (exact excerpt → fixed text, answer key
  changes, removed exercises) instead of rewriting the whole handout: the review step
  went from about 53 s to 17–33 s per handout, with the same kind of fixes (18 over 4
  topics, 1 of 19 exercises dropped).
- Quizzes of 4 or more questions are written as two parallel requests with different
  focus (rules and definitions; exceptions and applied cases), then de-duplicated.

### Added
- The latest lesson recording is saved as `last-recording.wav` in the data folder, to
  check a bad transcription against what the microphone captured.
- `quiz_check` example (quiz generation time); `stt_check` takes `KG_STT_CONTEXT`.

## [0.17.0] - 2026-10-28

### Added
- The tutor starts speaking while its answer is still arriving. Each complete sentence
  from the streamed reply goes straight to a speech queue and to the live captions; the
  final reply then replaces the captions and fills the whiteboard. In a live check the
  first sentence arrived after 2.7 s, against 10.8 s for the full reply that the voice
  used to wait for.
- Piper voice tuning in Settings: intonation expressiveness (`--noise_scale`) and rhythm
  variation (`--noise_w`), to adjust by ear with "Listen to a sample".
- Optional exercise verifier model (`MODEL_VERIFIER`, or Settings) for the blind solve of
  handout exercises; empty uses the correction model.
- `stream_check` example (first-sentence latency) and a multi-topic `handout_check`
  that reports corrections and dropped exercises.

### Measured
- Handout review over 4 topics in 3 subjects: 24 content corrections, 1 of 20 exercises
  dropped by the blind solve (5%). The reviewer also occasionally introduces small
  errors of its own (for example a misspelling), see ROADMAP.

## [0.16.0] - 2026-10-28

### Added
- Handout review pass. After a handout is written, the correction model reviews it
  against the edital excerpt and fixes or removes wrong statements, incoherent examples
  and broken exercises, without adding new content. Each exercise is then solved again
  without its answer key; exercises whose independent answer differs from the key are
  dropped.
- `handout_check` example: writes and reviews a handout on a topic where unreviewed
  handouts had wrong keys. In a live run on crase it made 7 corrections (including a
  contradictory rule about "à uma hora") and dropped 1 of 5 exercises.

### Changed
- Cached handouts written before the review pass are treated as stale, because the
  cache fingerprint now covers all three handout prompts.
- Writing a handout takes about twice as long (two extra model calls per topic).

## [0.15.2] - 2026-10-27

### Fixed
- Local Kokoro on Windows. The pinned sherpa-onnx executable reads command-line
  text in the active code page, which dropped accented Portuguese. The app now
  launches a sibling copy whose manifest requests UTF-8, and Dora, Alex and Santa
  can be installed and selected. Piper remains the default until a listening test.

### Changed
- README lists project contributors separately from font and icon credits.

## [0.15.1] - 2026-10-28

### Fixed
- Two clippy lints in the audio crate (`as_chunks` for Piper PCM decoding,
  `is_multiple_of` for WAV channel checks) that failed `-D warnings` on Rust 1.99.

## [0.15.0] - 2026-10-27

### Added
- Reuse previously imported board documents in another plan, preserving original
  questions/answers while classifying relevance against the destination syllabus.
- Opt-in scanned PDF OCR using `MODEL_VISION`, with local pure Rust page rasterization
  and an explicit notice before sending page images to the configured AI service.
- Daily active study time in Today, persisted in SQLite; pauses for unfocused windows,
  inactivity and suspend gaps, with calendar-day rollover.
- Whiteboard formulas and small flow diagrams, two avatar styles, voice-matched avatar,
  a dark logo variant and vector illustrations in empty states.
- Installed-voice gender filtering, sample playback/cancel, speed and pitch controls.
- Optional OpenAI-compatible speech service, off by default, with a text-sharing notice.
- Experimental Kokoro synthesis/model management with cancellable child processes.
- Streaming tutor client with SSE parsing and sentence callbacks; connecting it to
  the lesson speech queue remains pending.
- Optional CUDA Whisper build; Linux/macOS packaging scripts and native package CI.
- OpenAI Codex credit linked to Contributors in the README.

### Fixed
- Declared Rust minimum updated to 1.95 to match the existing desktop dependency.
- Voice child processes are stopped on cancellation, with bounded output and timeout.
- Model archives are installed through staging directories; incomplete Kokoro bundles
  are not treated as installed.

### Known limitations
- Local Kokoro is unavailable on Windows: the pinned sherpa executable crashes on
  accented UTF-8 arguments. The picker excludes it; Piper remains the local default.
- Live handout review stayed within the supplied syllabus but found factual/answer
  errors. Automatic factual correction remains pending; generated text needs review.
- CUDA hardware, native Linux/macOS packages, installer execution and the physical
  microphone loop were not validated in this release. See `docs/validation-v0.15.md`.

## [0.14.0] - 2026-10-27

### Added
- Kokoro listening evaluation example: nine Portuguese samples across Dora, Alex and
  Santa, with normalized text, process logs and cold-start timings. Uses locally
  supplied sherpa-onnx v1.12.20 and Kokoro v1.0 files; no automatic download or app
  voice selection yet. Setup and acceptance checks in `docs/kokoro-evaluation.md`.
- Handout requests include the original plan's syllabus-aware edital excerpt as
  reference data. Prompts constrain board material to that scope and reject document
  instructions. Empty edital context is rejected before a model call.

### Fixed
- Handout cache reuse now requires identical edital, exam, subject, topic, board
  reference and prompt. Legacy or changed-source entries are regenerated on the next
  export; unchanged entries still avoid model calls. Existing exported PDFs are kept.
- Cached-handout counts refresh after board changes and clear when no plan is active.
- Roadmap handoff reflects the integrated main branch and remaining validation work.

## [0.13.0] - 2026-10-27

### Added
- "Today" screen, where the app opens: greeting, study streak, reviews due, plan
  progress and week, one "next step" (reviews first, then the next lesson of the week)
  and the topics of the current week.
- First-run welcome with three steps: connect the AI (with connection test), download the
  voice models (one click, with progress), upload the edital. Shown only while there is no
  plan and it was never closed.
- Study activity per day (SQLite migration 4) for streaks; lessons, quizzes and reviews
  count. Milestones shown once: 3, 7, 14 and 30 days in a row; 25, 50, 75 and 100% of
  the plan.
- Empty states with an icon, one sentence and one action (Today, Plan, Quiz, Progress).
- Short fade when the screen changes; toasts fade in and out.
- Icons on the handout, export and toast close buttons.

## [0.12.0] - 2026-10-27

### Added
- Lessons look like a video call instead of a chat: a stage with the teacher, captions
  of the latest line, a name tag with the lesson phase, the student's tile with a live
  microphone meter, and a control bar (talk, interrupt, captions, transcript, end).
- Animated teacher drawn in code: breathes and blinks when idle, looks at the student and
  nods while listening, looks up with a thought bubble while thinking, and moves the
  mouth with the loudness of the voice being played.
- Whiteboard: with each answer the tutor writes one to three key points
  ("QUADRO: a | b" line, not spoken), which appear letter by letter.
- Transcript and typed chat in a side panel that can be closed.
- Keyboard: hold Space to talk, Esc interrupts the tutor.
- Playback measures the loudness of what is being heard, for lip-sync.

### Changed
- The microphone orb is replaced by the meeting controls.

## [0.11.0] - 2026-10-27

### Added
- Logo: an open book inside a speech bubble (study plus conversation), teal with a warm
  amber accent, as SVG (`crates/ui/assets/logo.svg`). The window icon and the
  executable icon are rendered from it.
- Fonts embedded in the app (SIL Open Font License): Literata for headings and the
  wordmark, Inter for text, with egui's fonts kept as fallbacks.
- Icons in the navigation (Lucide, ISC license), tinted to match the theme.
- Amber highlight color for counts and milestones.

### Changed
- Sidebar with the logo and wordmark; the selected screen shows its icon in the accent
  color and a semibold label.
- Progress bars are easier to see.
- Long topic titles on the Plan screen are cut with an ellipsis (full title on hover)
  instead of running under the chips.

## [0.10.0] - 2026-10-27

### Added
- Exam board material ("Banca" screen): upload past exams and board handouts as PDF.
  The AI extracts every question (multiple choice or Certo/Errado items) or summarizes
  each passage, and checks it against the plan's topics.
- Relevance filter: each item gets a topic and a level (relevant, partial, out of scope)
  with a short reason. Out-of-scope items (other roles, subjects not in the edital) are
  kept but never used; the student can move any item in or out by hand.
- Answer keys: attach the gabarito to an imported exam; answers are matched by question
  number (letters, or C/E for Certo/Errado).
- Quizzes start with real past exam questions on the topic and generate only the rest;
  a quiz made only of real questions needs no model call. Real questions are marked.
- Tutor, quiz, review and handout prompts receive the board's material on the topic.
- Plan screen shows how many past exam questions cover each topic.
- SQLite migration 3 (board documents, questions, excerpts).
- `board_check` example: edital → plan → past exam → answer key → quiz, against the real
  endpoint. Tested with a real Cebraspe exam: 50 questions read, 31 used, 19 set aside
  with the right reasons, answer key applied to 48.

### Changed
- `pdf_extract` warnings are hidden from the default log.

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
