# Roadmap — Know Giver

## Current Status

Version 0.20.0. Everything an agent can build and check without a human ear, a
microphone or other platforms is done. What remains open needs a person or hardware
(listed under "Not yet verified"). Each release is a short-lived branch merged into
`main` with an annotated tag.

### Handoff for the next agent

**Next up, in order:**
0. Conversation flow (v0.20.0): Parakeet with live captions and early end of turn.
   Measured per step on the user's machine before the change: silence 1.4 s, Whisper
   small 4.7 s, first tutor sentence 2.1 s, first Piper chunk ~0.4 s. Still to check
   with a real microphone: whether 0.7 s after a sentence cuts students off, and how
   often one-word answers need the Whisper check. Next candidates: keep Piper running
   between sentences (saves ~0.3 s per chunk), and barge-in (talking over the tutor),
   which needs echo cancellation or headphones.
1. Human checks (cannot be done by an agent): listening comparison Piper × Kokoro and
   choice of Piper expressiveness/rhythm defaults; live microphone loop with streamed
   speech; installer build and install test.
2. Tutor fact-checking: live lesson answers are not reviewed. A streamed reply on crase
   claimed "meio-dia"/"meia-noite" take no crase, which is wrong. Options: a lighter
   review of each finished reply shown as a correction, or stronger prompting.
3. Try a different model family as `MODEL_VERIFIER` (for example a local Ollama model)
   and compare the drop rate with `handout_check`.

Windows Kokoro voices are selectable. Piper stays the default until the listening
comparison.

**How the code is organized:** `app → ui → engine → {study, ai, audio, pdf, handout}`.
Engine owns state; slow work runs in workers and returns as events applied in
`Engine::poll`. UI text only via `locales/*.yml` (`t!`); tests fail on missing keys.
Repo text in English; user-facing app text pt-BR first. No `unwrap/expect/panic` outside
tests (workspace lints).

**How to verify:**
- `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- Screens to PNG: `KG_SNAPSHOT_DIR=<dir> cargo test -p kg-ui -- --ignored`
- Against the real AI (`.env`, which overrides global env vars):
  `cargo run -p kg-ai --example live_check`,
  `cargo run -p kg-engine --example e2e -- edital.pdf`,
  `cargo run -p kg-engine --example board_check -- edital.pdf prova.pdf gabarito.pdf`
- Voice: `cargo run --release -p kg-audio --example voice_check -- %LOCALAPPDATA%/KnowGiver/models small out.wav`

**Not verified by an agent:** live microphone loop end to end, installer build (needs
Inno Setup 6).

## Phase 1 — Foundation
- [x] Cargo workspace setup
- [x] Basic egui window and navigation
- [x] PDF upload and text extraction (`kg-pdf`, syllabus-aware excerpt for the prompt)
- [x] SQLite storage for plans and progress (`kg-study::store`, migrations)

## Phase 2 — Study Core
- [x] Study plan generation from edital (LLM, JSON mode, per-role, weeks × hours)
- [x] Display and navigation of the study plan (by week / by subject)
- [x] Progress tracking (topic status, lessons, quiz stats)
- [x] Basic quizzes (multiple choice, options shuffled, explanations)

## Phase 3 — Voice Lessons
- [x] Microphone capture (cpal, end-of-speech detection)
- [x] Local STT (Whisper on candle, pure Rust)
- [x] TTS response (Piper pt-BR, run as a child process)
- [x] Lesson conversation loop (STT → LLM → TTS, hands-free option)

## Phase 4 — Polish
- [x] Settings (AI endpoint, per-task models, voice, models, language)
- [x] Review system (AI recall question + graded answer, or self-grade)
- [x] Better UI feedback during listening/speaking (voice orb)
- [x] Export progress (Markdown report)

## Later
- [x] Spaced repetition (SM-2)
- [x] Multiple editals
- [x] Offline model management (download, cancel, remove)
- [x] Packaging and installers (`scripts/package.ps1`: portable zip; Inno Setup installer)

## Phase 5 — Exam board material and study handouts

### Optional upload of past exams and board material
- [x] Attach PDFs to a plan: past exams (provas anteriores), answer keys (gabaritos), board handouts
- [x] Store them in SQLite, linked to the plan (`board_documents`, kind: exam / key / material)
- [x] Extract questions from past exams with the LLM (statement, options, answer from the key)
- [x] Link each question to the plan's topics (LLM classification, kept in the database)
- [x] Relevance filter: keep only what matches the edital's syllabus
  - Each extracted question or material section is checked against the plan's topics, and
    gets a topic plus a relevance level (relevant / partly relevant / out of scope)
  - Out of scope (other roles, subjects not in this edital, topics removed from the
    syllabus) is set aside, not used by quizzes, tutor or handouts
  - After each upload, a summary: "42 questions used, 11 set aside (Direito Tributário is
    not in this edital)", with a list to review and move items back in by hand
  - [x] The same material can be reused in another plan; relevance is computed per plan
        while question wording, alternatives and known answers are preserved
- [x] Quizzes for a topic: real board questions first, then generated ones in the same style
- [x] Tutor, quiz, review and handout prompts use the board material on the topic
- [x] Show the board's emphasis per topic (how often it is asked) on the Plan screen
- [x] Scanned PDFs: opt-in OCR fallback with the vision model (`MODEL_VISION`)

### Written study material as PDF (apostilas)
- [x] Generate a written lesson per topic with the LLM: explanation, examples, common pitfalls, summary, practice questions with answers
- [x] Supply the stored edital excerpt and relevant board material to handout generation;
      constrain the prompt to the requested topic and invalidate caches when sources change
- [x] Review live handouts for syllabus fidelity, including multi-role and truncated editais
      (both checked examples stayed on topic)
- [x] Review pass for factual/exercise errors: content review plus blind solve of every
      exercise, keeping only exercises whose independent answer matches the key (v0.16.0)
- [x] Measure the residual error rate (v0.17.0): over 4 topics in 3 subjects the review
      made 24 corrections and the blind solve dropped 1 of 20 exercises (5%). Errors still
      slip through: both models agreed on "à semana passada" (should be "na semana
      passada"), and the reviewer itself wrote "advérbial" (should be "adverbial")
- [x] Optional verifier model (`MODEL_VERIFIER`) so a different family can solve blind
- [ ] Compare the drop rate with a verifier from another model family
- [x] Render to PDF in pure Rust (`typst` as a library), with embedded fonts
- [x] Export one topic, one subject or the whole plan as a single handout, with table of contents
- [x] Cache generated text in SQLite so regenerating the PDF is free; "write again" for a new version
- [x] Open or save the PDF from the Plan screen
- [x] Prepare everything at once (v0.19.0): optional on plan creation, or "Prepare
      everything now"; handouts plus a 10-question quiz bank per topic, then the plan PDF
      in Documents › Know Giver, with a progress panel and time estimate
- [ ] Run "prepare everything" on a real full plan (40+ topics) and record time, failures
      and rate-limit behaviour with 3 topics in parallel

## Phase 6 — Identity, UI/UX and meeting-style lessons

### Lesson as a meeting, not a chat
- [x] "Call" layout: a large stage with the tutor, a smaller tile for the student with the
      mic level, and a control bar at the bottom (mic, interrupt, captions, end lesson)
- [x] Animated teacher avatar drawn in code (egui painter, vector shapes, no video):
  - idle: breathing and blinking
  - listening: looks at the student and nods while the mic level moves
  - thinking: looks up, with a small "thinking" gesture
  - speaking: mouth driven by the loudness of the TTS audio (simple lip-sync), gestures
- [x] Live captions under the stage instead of chat bubbles; the full transcript moves to a
      side panel that can be closed
- [x] Whiteboard next to the tutor: the model returns a few key points with each answer,
      written on the board while it speaks
- [x] Small diagrams or formulas on the whiteboard
- [x] Keyboard: hold Space to talk, Esc to interrupt
- [x] Optional: a few avatar styles to choose from in Settings

### Visual identity
- [x] Logo: symbol + wordmark, as SVG: an open book inside a speech bubble
- [x] Logo variant for dark backgrounds
- [x] App icon, installer and window title bar generated from the logo
- [x] Brand palette: keep the calm teal and add a warm accent (amber) for highlights,
      progress and achievements
- [x] Typography: embedded open-license fonts (Literata for headings, Inter for text)
- [x] Icon set (Lucide SVG rendered with resvg, pure Rust) for navigation

### UX
- [x] "Today" home screen: what to study today, reviews due, next step, streak
- [x] Time studied per day on the Today screen
- [x] First-run onboarding: AI key → speech models → upload edital → first plan
- [x] Empty states with an icon and a single clear next step
- [x] Vector illustrations for empty states
- [x] Streaks and gentle milestones (no gamified noise)
- [x] Micro-animations: screen transitions, progress filling, toast entry
- [x] Review the whole flow for clicks saved (start a lesson straight from Today)

## Phase 7 — Natural voice, male and female

Piper's official Brazilian Portuguese voices (faber, cadu, jeff, edresson) are all male
and sound somewhat robotic. A female voice needs another engine.

### Quick wins on the current voice
- [x] Longer pauses between sentences (`--sentence_silence`), slightly slower default speed
- [x] Expose Piper `noise_scale` / `noise_w` in Settings (expressiveness, rhythm)
- [ ] Choose their defaults by listening tests (needs a person)
- [x] Text normalization before speech: legal abbreviations, ordinals, dates, law numbers
      and years, roman numerals after "inciso"/"título", percentages, money, brackets
- [x] Tutor prompt: short spoken sentences, no parentheses or lists, natural connectors
- [x] Softer playback: fades between chunks, loudness normalization, gentle
      high-frequency roll-off

### More natural engine (local)
- [x] Reproducible local listening harness (`kokoro_check`) for the three Portuguese
      voices, pinned runtime/model instructions, timings and logs
- [x] Kokoro-82M (Apache-2.0, runs on CPU): Brazilian Portuguese voices `pf_dora`
      (female), `pm_alex` and `pm_santa` (male), selectable on Windows since v0.15.2
- [x] Run it like Piper as a cancellable child process, with model installation/removal
- [x] Resolve pinned runtime's accented-text crash on Windows before enabling Kokoro there
- [ ] Listening test against Piper on real tutor answers before making it the default
      (needs a person)

### Optional cloud voices (off by default)
- [x] OpenAI-compatible `/v1/audio/speech` backend: supports the contract used by OpenAI, LiteLLM, or a local
      Kokoro-FastAPI server
- [x] Clear notice that the tutor's text is sent to that service

### Voice settings
- [x] Choose **female or male**, then the voice, filtered by what is installed and supported
- [x] "Listen to a sample" button for the selected voice
- [x] Speed and pitch sliders
- [x] The teacher avatar (Phase 6) matches the chosen voice

## Not yet verified
- Kokoro Portuguese listening comparison against Piper on real tutor answers. Accented
  text now reaches the Windows runtime; subjective quality is still open.
- Handout factual correctness: the review pass reduces errors (see Phase 5) but does not
  eliminate them.
- Tutor answers in lessons are not fact-checked.
- Voice loop with a real microphone and speakers.
- Installer build/run; Inno Setup 6 compiler was prepared locally but the final release
  installer was not built before the user requested closing work.
- CUDA execution and native Linux/macOS packages (tooling is implemented).

## Next ideas
- [x] Streaming AI client with silent whiteboard metadata and sentence callbacks.
- [x] Stream sentences into lesson playback before the full reply arrives (v0.17.0:
      first sentence at 2.7 s instead of 10.8 s in a live check).
- [x] Optional GPU (CUDA) build of candle; benchmark on supported hardware remains pending.
- [x] OCR fallback for scanned editais (vision model).
- [x] Linux/macOS native packaging scripts and CI; native execution remains unverified.
