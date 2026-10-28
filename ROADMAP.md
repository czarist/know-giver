# Roadmap — Know Giver

## Current Status

All planned phases are implemented. Verified against the real GLM endpoint and a real
edital (TJ-CE 2023, 32 pages → 7 subjects, 92 topics in ~17 s). Speech recognition was
verified offline with Whisper base and small. Still to verify by hand on a machine with
microphone and speakers: the full voice loop and Piper speech (see "Not yet verified").

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
  - [ ] The same material can be reused in another plan; relevance is computed per plan
        (today: upload the file again in the other plan)
- [x] Quizzes for a topic: real board questions first, then generated ones in the same style
- [x] Tutor, quiz, review and handout prompts use the board material on the topic
- [x] Show the board's emphasis per topic (how often it is asked) on the Plan screen
- [ ] Scanned PDFs: OCR fallback with the vision model (`MODEL_VISION`)

### Written study material as PDF (apostilas)
- [x] Generate a written lesson per topic with the LLM: explanation, examples, common pitfalls, summary, practice questions with answers
- [ ] Use the edital and the board material as the source, never content outside the edital
      (handouts now receive the board material on each topic)
- [x] Render to PDF in pure Rust (`typst` as a library), with embedded fonts
- [x] Export one topic, one subject or the whole plan as a single handout, with table of contents
- [x] Cache generated text in SQLite so regenerating the PDF is free; "write again" for a new version
- [x] Open or save the PDF from the Plan screen

## Phase 6 — Identity, UI/UX and meeting-style lessons

### Lesson as a meeting, not a chat
- [ ] "Call" layout: a large stage with the tutor, a smaller tile for the student with the
      mic level, and a control bar at the bottom (mic, interrupt, captions, end lesson)
- [ ] Animated teacher avatar drawn in code (egui painter, vector shapes, no video):
  - idle: breathing and blinking
  - listening: looks at the student and nods while the mic level moves
  - thinking: looks up, with a small "thinking" gesture
  - speaking: mouth driven by the loudness of the TTS audio (simple lip-sync), gestures
- [ ] Live captions under the stage instead of chat bubbles; the full transcript moves to a
      side panel that can be closed
- [ ] Whiteboard next to the tutor: the model returns a few key points (or a small diagram
      or formula) with each answer, written on the board while it speaks
- [ ] Keyboard: hold Space to talk, Esc to interrupt
- [ ] Optional: a few avatar styles to choose from in Settings

### Visual identity
- [ ] Logo: symbol + wordmark, as SVG (concepts to explore: open book that is also a speech
      bubble; a "K" made of a book and a sound wave), with light and dark versions
- [ ] App icon, installer and window title bar generated from the logo
- [ ] Brand palette: keep the calm teal and add a warm accent (amber) for highlights,
      progress and achievements
- [ ] Typography: embedded open-license fonts (e.g. a humanist serif for headings, a
      readable sans for text) instead of egui's default font
- [ ] Icon set (SVG rendered with resvg, pure Rust) for navigation and actions

### UX
- [ ] "Today" home screen: what to study today, reviews due, next lesson, time studied
- [ ] First-run onboarding: AI key → speech models → upload edital → first plan
- [ ] Empty states with illustrations and a single clear next step
- [ ] Streaks and gentle milestones (no gamified noise)
- [ ] Micro-animations: screen transitions, progress filling, toast entry
- [ ] Review the whole flow for clicks saved (e.g. start a lesson straight from Today)

## Phase 7 — Natural voice, male and female

Piper's official Brazilian Portuguese voices (faber, cadu, jeff, edresson) are all male
and sound somewhat robotic. A female voice needs another engine.

### Quick wins on the current voice
- [x] Longer pauses between sentences (`--sentence_silence`), slightly slower default speed
- [ ] Tune Piper `noise_scale` / `noise_w` by listening tests
- [x] Text normalization before speech: legal abbreviations, ordinals, dates, law numbers
      and years, roman numerals after "inciso"/"título", percentages, money, brackets
- [x] Tutor prompt: short spoken sentences, no parentheses or lists, natural connectors
- [x] Softer playback: fades between chunks, loudness normalization, gentle
      high-frequency roll-off

### More natural engine (local)
- [ ] Kokoro-82M (Apache-2.0, runs on CPU): Brazilian Portuguese voices `pf_dora`
      (female), `pm_alex` and `pm_santa` (male)
- [ ] Run it like Piper, as a child process (candidate: sherpa-onnx's prebuilt TTS tool), or
      in-process with a pure Rust ONNX runtime if quality and speed allow (to evaluate)
- [ ] Listening test against Piper on real tutor answers before making it the default

### Optional cloud voices (off by default)
- [ ] OpenAI-compatible `/v1/audio/speech` backend: works with OpenAI, LiteLLM, or a local
      Kokoro-FastAPI server
- [ ] Clear notice that the tutor's text is sent to that service

### Voice settings
- [ ] Choose **female or male**, then the voice, filtered by what is installed
- [ ] "Listen to a sample" button for each voice
- [ ] Speed and pitch sliders
- [ ] The teacher avatar (Phase 6) matches the chosen voice

## Not yet verified
- Voice loop with a real microphone and speakers.
- Piper download and speech (downloading and running `piper.exe` needs the user's go-ahead).
- Installer build (needs Inno Setup 6 installed).

## Next ideas
- Stream tutor answers so speech starts before the full reply arrives.
- Optional GPU (CUDA) build of candle for faster Whisper small.
- OCR fallback for scanned editais (vision model).
- Linux/macOS packages.
