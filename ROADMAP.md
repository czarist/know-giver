# Roadmap — Know Giver

## Current Status

Version 0.28.0. Everything an agent can build and check without a human ear, a
microphone or other platforms is done. What remains open needs a person, hardware or
the real AI service (listed under "Not yet verified"). Each release ends with a
CHANGELOG entry, the workspace version bump and an annotated tag `vX.Y.Z` on `main`
(feature branches are optional; recent releases were committed to `main` directly).

### Handoff for the next agent

**Next up, in order:**
0. Lessons (v0.27.0): run a Normal and a Full lesson on a prepared topic with the
   real model. The tutor now gets each step's key points and the quiz's questions; check
   it covers them before `ETAPA:`, that replies stay short, and how long lessons take.
1. Student turns (v0.26.2): the log of 2026-10-28 showed every turn ending after
   about 450 ms of quiet (`quiet_ms=453`), because Parakeet ends almost every partial
   with a full stop. The wait is now a setting (`turn_pause`, 1 s default, +0.8 s
   without a finished sentence), speech counts from level 0.0025, and any English
   word without a Portuguese one asks Whisper. The owner's recording was exactly 0.0
   between words and about 0.005 during speech: a driver noise suppressor and a low
   microphone gain. Check new logs (`end of turn ... quiet_ms`) before tuning more.
2. Voices (v0.28.0): Kokoro now stays loaded in the app through
   `sherpa-onnx-c-api.dll` (module `kg_audio::kokoro_ffi`, the only `unsafe` code;
   the workspace lint is `deny`), 1.2–2.3 s per sentence. English passages marked
   `[en]…[/en]` by the tutor are read by an English Kokoro voice, still as a separate
   program (Kokoro's language is set when the model loads; a second loaded copy would
   double the memory). Check with the owner: memory use with Kokoro loaded, how the
   English voice sounds, and whether the tutor marks English well.
   Mistakes notebook, lesson drafts and recaps, spoken reviews (v0.28.0): check with
   real use that a resumed lesson picks up naturally and that reviews read aloud.
3. Check v0.26.0 with the real model and a person: does the tutor write `ROTEIRO:`
   in its first answer, mark `ETAPA: n` at sensible moments and close with `FIM` after
   the final review? Does the handout writer pick useful `image` titles? Read a
   whole-plan handout (100+ pages) in the reader and watch memory and scrolling.
4. Microphone (v0.25.0): one stream open for the whole lesson, speech threshold 0.005,
   recordings made louder and their pauses shortened before STT, English-looking Parakeet output sent to
   Whisper (short window, token budget). Ask the user for `know-giver.log` (data
   folder) and `last-recording.wav` before tuning further.
5. Conversation flow (v0.20.0–v0.22.0): Parakeet with live captions, end of turn when
   the caption is stable (0.45 s after a sentence, 1.2 s otherwise), a spoken "Hum."
   while the answer is written, continuation while the tutor thinks, barge-in with echo
   recognized by comparing words, Piper kept running. Measured before: silence 1.4 s,
   Whisper small 4.7 s, first tutor sentence 1.5–2.2 s (server time; reusing the
   connection did not help; glm-4.5-flash/air are not faster), first Piper chunk
   ~0.6 s. The app logs "end of turn" and "first tutor sentence" timings: ask the user
   for those lines before tuning further. Still to check with a real
   microphone and speakers: false interruptions from echo (raise the word threshold if
   they happen), CPU load of checks while the tutor speaks, and how often one-word
   answers need the Whisper check.
6. Human checks (cannot be done by an agent): listening comparison Piper × Kokoro and
   choice of Piper expressiveness/rhythm defaults; live microphone loop with streamed
   speech; installer build and install test.
7. Tutor fact-checking: live lesson answers are not reviewed. A streamed reply on crase
   claimed "meio-dia"/"meia-noite" take no crase, which is wrong. Options: a lighter
   review of each finished reply shown as a correction, or stronger prompting.
8. Try a different model family as `MODEL_VERIFIER` (for example a local Ollama model)
   and compare the drop rate with `handout_check`.

Windows Kokoro voices are selectable (Dora, Sara, Heart, Emma, Alex, Santa), plus the
Dii Piper voice (CC BY-NC-ND, personal use). The teacher picked in Settings picks the
voice; Piper stays the default until the listening comparison.

**How the code is organized:** `app → ui → engine → {study, ai, audio, pdf, handout}`.
The handout crate lays pages out with Typst and also serves the in-app reader
(`kg_handout::paginate`: page images through `typst-render`, text boxes for selection).
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
- [x] Pictures in handouts (v0.26.0): one per topic and up to two per topic in sections,
      chosen by the writer and fetched from Wikipedia through the picture cache
- [x] Exercise cards and an answer key with a quick answer grid (v0.26.0)
- [x] Several roles in one edital (v0.24.0): roles are detected, the student picks one,
      and only that role's specific syllabus reaches the plan prompt

### Reading inside the app (v0.26.0)
- [x] Handouts screen: every generated handout with its reading progress
- [x] Reader: pages exactly as the PDF, continuous scroll, zoom, rendered in the
      background and kept near the current page
- [x] Highlighter in four colors by dragging over text; notes; side list that jumps to
      the page; highlights stored in SQLite
- [x] Last page and pages read are remembered; progress bar per handout
- [x] Save a PDF with the highlights drawn on it
- [x] A finished handout opens in the reader instead of an external program
- [x] Navigation (v0.27.0): previous/next page, typed page number, Page Up/Down,
      Home/End, and a Contents tab built from where each heading lands
- [x] Move the page by dragging (Move tool, or a drag away from text) and middle-click
      autoscroll (v0.27.0)
- [x] Remove a highlight from the side list (v0.27.0)
- [ ] Select across pages (today a highlight stays within one page)
- [ ] Use highlights in lessons, quizzes and reviews (the tutor revisits what you marked)
- [ ] Open PDFs that were not generated by the app (needs a PDF renderer; `hayro` is the
      pure-Rust candidate)

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
- [x] Teachers instead of avatar styles (v0.24.0): Helena, Lia, Rafael, Otávio, ink
      portraits ("Lorelei", CC0) that breathe, blink, nod and lip-sync; the tutor uses
      the teacher's name and gender
- [x] Richer whiteboard (v0.24.0): tables, timelines, mind maps, exam tips, examples and
      Wikipedia pictures
- [x] Lesson plan and an end (v0.26.0): three to five steps plus a final review, from the
      handout headings or the tutor's `ROTEIRO:` line; "Step n of m" and a completion card
      that offers the quiz
- [x] Lessons with content (v0.27.0): every handout section, grouped to the lesson
      length (Quick 3 / Normal 5 / Full 8 steps); each step carries its key points; the
      quiz's questions are given to the tutor so the lesson covers them
- [x] Taken lessons stick (v0.27.1): saved when the tutor closes the lesson, ticked in
      Today and the plan, "Retake lesson", and no longer suggested again

### Visual identity
- [x] App icon (v0.25.0): the owner's engraving of Thoth as a baboon under the crescent
      moon, on a jade tile (`tools/icon.py`); used for the window, `.ico`, sidebar,
      cover plates and README
- [x] App icon, installer and window title bar generated from it
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
- [x] Captions follow playback: a sentence appears when its audio starts (v0.26.1)
- [x] Kokoro with up to four threads (v0.26.1); the int8 bundle was slower on CPU
- [ ] Keep Kokoro loaded between sentences (today each sentence starts the runtime
      and loads the 325 MB model again, about 2 s)
- [ ] English voices (Kokoro English speakers, Piper en_US/en_GB)
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
- Lesson depth with the real model (v0.27.0): does the tutor cover every key point and
  every quiz subject before `ETAPA:`, and how long do Normal and Full lessons take?
- Reader autoscroll and dragging with a real mouse (v0.27.0).
- Long spoken answers with the owner's microphone after the turn changes (v0.26.2):
  no cut-offs mid-sentence, and fewer wrong verbs.
- Captions in step with the voice in a live lesson, for Piper and Kokoro teachers,
  including interruptions and a voice failure mid-reply (v0.26.1).
- Lesson plan with the real model: `ROTEIRO:` / `ETAPA:` / `FIM` lines (v0.26.0).
- Reader with a long whole-plan handout: memory, scrolling and render times.
- Handout pictures chosen by the live writer, and how often Wikipedia has one.
- Microphone changes of v0.25.0 with the user's microphone (quiet speech, echo).
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
