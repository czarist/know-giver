# Roadmap — Know Giver

## Current Status

Version 0.33.1. Everything an agent can build and check without a human ear, a
microphone or other platforms is done. What remains open needs a person, hardware or
the real AI service (listed under "Not yet verified"). Each release ends with a
CHANGELOG entry, the workspace version bump and an annotated tag `vX.Y.Z` on `main`.
Work happens on short-lived task branches from `dev`, merged into `dev`; the owner merges
`dev` into `main` on GitHub (see "Git workflow" in CLAUDE.md). Up to v0.31.2, releases
were committed to `main` directly.

### Handoff for the next agent

**Before building:** the owner's PC powers off under long full-CPU builds (five
unexpected shutdowns on 2026-10-29, no bug check and no hardware errors logged: a power or
thermal cut). Build and test with `-j 2` to `-j 4` and matching `--test-threads`; a
power loss mid-build corrupted `target/debug` and the git index once. The release
executable on disk is still v0.30.0: run `cargo build --release -j 4` when it is safe.

**Next up, in order:**
000. English prompts (v0.30.0): run `KG_PROMPT_LANG=en cargo run -p kg-ai --example
   live_check` and a lesson with the app in English; check the tutor keeps the markers
   (`QUADRO:`, `ETAPA:`, `FIM`) and that handouts and quizzes come out in English.
00. Written answers (v0.29.0): run [docs/validation-discursive.md](docs/validation-discursive.md)
   with the owner's vision model and real handwritten pages: legible, crossed-out,
   several pages and bad photos. Check the transcript keeps the student's errors, that
   the photo thresholds (`BLUR_EDGE`, dark, contrast in `kg_engine::discursive::images`)
   suit real phone photos, and how the grade compares with a human correction.
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

Planned, not started: managed local AI (Phase 8) — hardware diagnosis, model install,
LiteLLM as a local proxy and optional gpu-unlock. Start with its investigation (8.1).

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

## Phase 8 — Managed local AI (planned, not started)

**Goal:** the student picks "run the AI on this computer" and gets a working setup without
starting servers, downloading model files or typing an API key. The app checks the machine,
recommends a model, installs it after consent and runs the needed processes in the
background. When no supported setup meets the minimum, local mode stays off and the app
says why and what could change that.

**Status:** planning only. None of the components below ship today. What exists: the
student can point Settings → AI at any OpenAI-compatible endpoint, including a local
Ollama (`AiConfig` default `http://localhost:11434/v1`), with one model per task
(`ModelSet`) and a separate vision model. That manual path stays as it is.

### Target architecture

```text
Know Giver (Rust) ──HTTP, loopback──> LiteLLM (proxy) ──> llama.cpp server ──> model files (GGUF)
                                                        ▲
gpu-unlock (optional, boot time, Linux or Windows) ── prepares the GPU, never in the request path
```

- **Know Giver** keeps all orchestration in Rust: diagnosis, catalog, downloads, process
  supervision, health checks and settings live in `kg-engine` workers and report through
  `Event`s, like model downloads and the Parakeet server today. The AI client keeps
  talking OpenAI-compatible HTTP; local mode only changes the base URL, the key and the
  model aliases.
- **LiteLLM** is the gateway: one stable endpoint, per-task model aliases, a key, request
  limits and timeouts. It does not load or run models.
- **llama.cpp** (`llama-server`) loads the model and runs it on CPU or GPU (8.3).
- **gpu-unlock** belongs to hardware preparation. Local AI must work with it absent,
  unsupported or failed.

### Exceptions to the pure-Rust preference

The owner authorized these external projects. Each is integrated as published, not
rewritten in Rust, and runs outside the app process; orchestration stays in Rust. This is
not a general permission for dependencies in other languages: any further one needs its own
recorded decision.

- **LiteLLM** — <https://github.com/BerriAI/litellm>. Python, embedded in the installer
  and run as a managed child process bound to loopback. Use only the open-source part;
  features under the LiteLLM commercial license stay off. Confirm the license of the
  pinned version before shipping.
- **llama.cpp** — <https://github.com/ggml-org/llama.cpp>. C/C++ (MIT), the inference
  engine: its `llama-server` binary runs as a child process, like the sherpa-onnx runtime
  today. Not linked into the app.
- **gpu-unlock** — <https://github.com/Zagitta/gpu-unlock> on Linux (kernel module, C,
  GPL-2.0-only) and its Windows port **Windows-GPU-UNLOCK** —
  <https://github.com/impacifist/Windows-GPU-UNLOCK> (UEFI pre-boot tool, GPL-2.0-only).
  Optional, only on an explicit user action through the system's administrator prompt.
  Not bundled in the installer; NVIDIA firmware is never redistributed.

### Dependencies

- Existing: the model download code (`kg_audio::models::ModelStore`/`Fetcher`, which has
  no checksum or resume today), engine workers and events, `Settings`/`AiConfig`, the rule
  that images go only to a vision model, the vision probe (`check_vision`), the i18n
  system, and the packaging scripts (`scripts/package.ps1`, `scripts/package-unix.sh`).
- Order inside the phase: 8.1 → 8.2 and 8.3 in parallel → 8.4 → 8.5 → 8.6 → 8.8.
  8.7 depends only on 8.2 and 8.8's benchmark and can come last or never.
- Related items elsewhere: the optional CUDA build of candle (Whisper on the GPU shares
  VRAM with the model), "CUDA execution … unverified" under Not yet verified, and the idea
  of a local model as `MODEL_VERIFIER` (Next up #8), which a managed install would make
  easy to try.

### 8.1 Investigation
- [ ] Measure what the app already uses while a lesson runs: RAM and VRAM for Whisper or
      Parakeet, Kokoro or Piper, the reader and the UI, on Windows and Linux. These are
      the reserve the model must leave free.
- [ ] Write per-task needs from the current prompts: context length (edital excerpts,
      handouts with board material), output length (study plan, handout), JSON reliability
      (quiz, correction, written-answer grades), streaming latency (tutor first sentence)
      and vision (scanned editais, handwritten pages).
- [ ] Shortlist Portuguese-capable open models per task and size class, with license,
      quantizations and vision support; record sources and dates.
- [ ] Measure the installer with LiteLLM and llama.cpp embedded: size, start time,
      memory, and Windows antivirus behaviour with the embedded Python.

### 8.2 Hardware diagnosis
- [ ] Detect OS and architecture; CPU model, cores and instruction sets (AVX2, AVX-512,
      NEON); total and available RAM.
- [ ] Detect GPUs: vendor, device ID, VRAM total and free, driver version. NVIDIA, AMD,
      Intel and Apple, with a source per platform (to investigate: NVML, DXGI, Metal, sysfs,
      or llama.cpp's own device listing).
- [ ] Detect which inference backends really work (CUDA, Vulkan, ROCm/HIP, SYCL, Metal,
      CPU) by asking the chosen engine to list and initialize devices, not by guessing from
      the GPU name.
- [ ] Check free disk space for download, unpacking and running (download and installed
      size counted separately).
- [ ] Subtract the app's own use (8.1) and a safety margin from available memory.
- [ ] Keep a fingerprint of hardware, driver, engine and model; when it changes, run the
      diagnosis again before starting local mode.
- [ ] Keep a versioned compatibility matrix in the repo (OS, hardware, driver, backend,
      engine version, model, quantization), each row marked **verified**, **experimental**
      or **incompatible**, with who tested it and when.
- [ ] Evaluate NVIDIA without unlock, AMD, Intel, Apple Silicon and CPU-only on their own
      backends. A missing gpu-unlock never blocks local mode by itself.

### 8.3 Inference engine: llama.cpp
Chosen 2026-10-29 for its hardware coverage (CUDA, Vulkan, ROCm/HIP, SYCL, Metal, CPU),
prebuilt Windows/Linux/macOS binaries, GGUF models, an OpenAI-compatible server, vision
models and grammar-constrained JSON. Alternatives considered: `mistral.rs` (Rust, narrower
hardware), Ollama (its own model store overlaps an app-managed catalog), candle in process
(serving and vision would be ours to build), vLLM-style server stacks (not for a desktop).
- [ ] Pin one `llama.cpp` release; list which of its builds the app uses per platform
      and backend, with SHA-256 in the manifest (see Decisions).
- [ ] Use the engine's device listing for the backend check in 8.2.
- [ ] Check vision models (multimodal projector files), JSON grammar and streaming through
      LiteLLM with the app's real prompts.
- [ ] Settings per model: context size, GPU layers, threads (leave cores for STT/TTS),
      batch size; from the catalog, not typed by the student.

### 8.4 LiteLLM integration
- [ ] Generate the LiteLLM config from the app: only local engine backends, one alias per
      task (`kg-study-plan`, `kg-tutor`, `kg-vision`, …) mapped to installed models, no
      external providers in local mode.
- [ ] Bind LiteLLM and the engine to `127.0.0.1` on free ports; protect LiteLLM with a
      random master key kept in the app's settings and the engine with its own key or a
      loopback-only socket; turn off the admin UI, telemetry, callbacks and spend
      database.
- [ ] Start, health-check (LiteLLM and engine), stop and restart from a supervisor worker in
      `kg-engine`; restart with a limit and backoff; stop both processes on app exit and
      clean up leftovers on the next start, as the Parakeet server does.
- [ ] Pin LiteLLM and llama.cpp in the manifest; updates only through an app release,
      with the same checks as a first install.
- [ ] Map errors (engine not ready, out of memory, model missing, timeout) to `AiError`
      kinds the UI already translates, with new keys where needed.

### 8.5 Model catalog and installation
- [ ] The model part of the `local-ai.toml` manifest (see Decisions): source and revision, license,
      SHA-256, download and installed size, quantization, context, memory needed at that
      context (weights + KV cache + engine overhead), and capabilities per task (text,
      structured JSON, tools if ever needed, vision).
- [ ] Recommend from the diagnosis: Portuguese quality on the app's tasks, latency,
      context and running memory with room for STT/TTS. Fitting the file into VRAM is not
      enough.
- [ ] After the student accepts the size: download with progress, cancel, resume
      (HTTP ranges) and SHA-256 check, staged so an interrupted install never looks
      installed; remove frees the space. Extend the existing downloader rather than add a
      second one, so speech models gain checksums and resume too.
- [ ] Per-feature availability: a text-only setup enables lessons, plans, quizzes and
      handouts, and marks image features (scanned editais, written answers) unavailable
      with the reason. A text model never receives images.
- [ ] Optional vision model as a separate install, checked with the existing vision probe.

### 8.6 Setup experience
- [ ] A clear choice in Settings → AI and onboarding: external API or on this computer.
      The external settings are kept, so switching back needs no retyping.
- [ ] Diagnosis and recommendation before anything is installed: what was found, the
      recommended model, download and disk size, expected speed, what will not work
      (for example, no vision).
- [ ] Local mode sends no AI request outside the machine. Downloads of components and
      models come only from pinned sources. Wikipedia pictures and update checks stay on;
      Settings lists the services the app still reaches.
- [ ] No automatic fallback to an external API; if the local service fails, the app says
      so and offers retry, a smaller model, or switching mode on purpose.
- [ ] Status in the app: starting, ready, busy, stopped, failed, with the reason.
- [ ] All new UI text through `locales/*.yml` in every locale.
- [ ] Heavy work (diagnosis, downloads, test runs, process start) in workers; the UI
      stays responsive.

### 8.7 Optional gpu-unlock
Upstream, checked 2026-10-29:
- **Linux, gpu-unlock 2.0.0:** kernel module loaded at boot with a kernel command-line
  parameter. Ampere (RTX 30) stable, Ada (RTX 40) and Turing (RTX 20) experimental,
  Blackwell (RTX 50) unsupported, laptops mixed.
- **Windows, Windows-GPU-UNLOCK 0.3.1 (source-only pre-release):** a UEFI tool picked from
  a boot entry before Windows starts. It needs Secure Boot off, BitLocker off, a separate
  FAT32 EFI partition, Python and clang, and firmware files the user obtains. Success is
  reported per card and system (RTX 3060–3090 Ti), not per model.
- Both remove a cap on FP16/BF16/TF32 tensor math. Upstream's own reports show little gain
  for quantized llama.cpp models (0–15%) and none for token decoding, which dominates
  tutor replies.

Tasks:
- [ ] Check exact compatibility against the pinned upstream versions: PCI device IDs,
      OS, kernel and headers or UEFI and boot setup, driver, Secure Boot and disk
      encryption. Never assume every CUDA card qualifies.
- [ ] Explain before enabling: administrator rights, boot changes, a reboot, what changes
      and how to undo it; then run the allowed steps through the administrator prompt
      after consent.
- [ ] Enable only on an explicit request, never silently.
- [ ] Verify after reboot (Linux kernel log; the Windows tool's own check). Offer disable
      (Linux: remove the boot parameter; Windows: boot the normal Windows entry and the
      port's restore script), uninstall, and recovery steps.
- [ ] Benchmark the app's real workload with and without it on llama.cpp and the chosen
      model; recommend it only with a measured gain. Never promise a speedup.
- [ ] If it is unavailable or fails, local mode runs normally when it is otherwise viable.

### 8.8 Validation
- [ ] Minimum profile per feature (tutor, study plan, quiz/correction, handout, vision),
      set from measurements on real machines, not universal RAM/VRAM numbers.
- [ ] A short trial run after install and after any fingerprint change: model loads, peak
      memory with STT/TTS loaded, time to first token, tokens per second, one valid JSON
      answer, and the vision probe when a vision model is installed.
- [ ] Out-of-memory and start failures: detect them, stop cleanly, explain, and offer a
      smaller model or shorter context.
- [ ] Tests for the supervisor, catalog checks, config generation (loopback only, no
      external providers) and availability rules with simulated processes.
- [ ] Fill the compatibility matrix with at least one verified row per supported backend.
- [ ] Measure LiteLLM's overhead against calling `llama-server` directly (latency to first
      token, memory) and record it.

### Decisions (2026-10-29, owner)
- **Inference engine:** `llama.cpp` server, with an exception like LiteLLM's.
- **LiteLLM packaging:** embedded in the installer (pinned Python runtime and hashed
  wheels, no Docker, nothing installed system-wide).
- **LiteLLM stays** as the proxy even with a single local engine; its overhead is
  measured and recorded (8.8), not a reason to drop it.
- **gpu-unlock on Linux and Windows:** the Windows port is included in the exception.
- **Privileged steps:** the app asks for consent, then calls the system's administrator
  prompt (UAC on Windows, `pkexec`/polkit on Linux). Steps the app must not do for the
  student, such as turning off Secure Boot or BitLocker or changing firmware settings, are
  explained with instructions and checked afterwards, never done silently.
- **Network features stay on in local mode:** Wikipedia pictures in handouts and update
  checks keep working. The local-mode promise covers AI requests; Settings says which
  other services the app still reaches.
- **Catalog and matrix: one manifest, used like a lockfile** (recommended by the agent;
  the owner wants components installed like package dependencies):
  - A versioned `local-ai.toml` in the repo lists every pinned component (LiteLLM,
    llama.cpp builds per platform/backend, models) with source, revision, license,
    SHA-256 and sizes, plus the compatibility matrix rows.
  - The packaging scripts read it to fetch and verify what the installer embeds: LiteLLM
    and the llama.cpp CPU and Vulkan builds.
  - The app embeds the same file at build time (`include_str!`). After diagnosis it
    fetches what is too large or too specific for the installer the same way, with
    consent: models, and the CUDA or ROCm build of llama.cpp when such a GPU is found.
  - Changes only through a release, so each version ships the set that was tested with it.
    A generated `docs/local-ai-compatibility.md` shows the matrix for people.

### Still open
- Installer size with LiteLLM and llama.cpp embedded, and Windows antivirus behaviour with
  the embedded Python; measure in 8.1.
- How far Windows-GPU-UNLOCK can be guided: it needs Secure Boot off, BitLocker off, a
  separate FAT32 EFI partition and firmware files the user obtains; decide whether the
  app offers it on Windows or only links to its instructions after the benchmark.

### Acceptance criteria
- **Compatible hardware:** diagnosis recommends a catalog model; after consent it downloads,
  verifies, starts LiteLLM and the engine on loopback, passes the trial run, and a lesson,
  a quiz and a study plan work with nothing sent outside the machine.
- **GPU without gpu-unlock support:** the GPU is used through its normal backend, or CPU if
  none works; gpu-unlock is shown as unavailable with the reason and nothing else is
  blocked.
- **Insufficient computer:** local mode stays disabled with the measured reason (for example
  "4 GB free, the smallest supported setup needs 6 GB with voice loaded") and the
  possible action (free memory, use an external API); no partial install is left.
- **Interrupted download:** resumes from where it stopped or restarts cleanly; a corrupt or
  partial file never counts as installed; cancel removes the temporary files.
- **Service failure:** a crash or failed start of LiteLLM or the engine shows a clear
  status, restarts within the limit, then stops with an explanation and actions; no
  silent switch to an external API; no orphan processes after exit.
- **No vision model:** text features work; scanned-edital OCR and written-answer reading show
  "needs a vision model" with the option to install one; no image ever reaches a text
  model.

## Not yet verified
- Updates and one-line installs (v0.33.0): need the first published release.
  - The release job, its asset names and `SHA256SUMS.txt` on a real tag.
  - `scripts/install.sh` on macOS (Apple silicon) and Linux, and `scripts/install.ps1` on
    Windows, from the README commands.
  - In-app update end to end: the bar, download, checksum, silent installer on Windows (it
    closes the app and opens the new version) and the install script on macOS and Linux.
  - No Intel macOS or ARM Linux package yet: those systems get the release page.
  - Desktop shortcut question and uninstallers (v0.33.1): the Inno uninstaller's study-data
    question on Windows, and `uninstall.sh` on macOS and Linux.
- Chinese and Japanese (v0.32.0): a native speaker should review each language.
  - Japanese, Simplified and Traditional UI texts and prompts. Traditional was converted
    from Simplified with OpenCC (Taiwan phrasing) and corrected by hand; Hong Kong usage
    differs in places.
  - Lessons, quizzes, handouts and grades with a real model in each language.
  - Fonts on macOS and Linux (the font paths there are not tested).
- Mainland China AI services (v0.32.0): endpoints, key pages and model names of Zhipu
  BigModel, Alibaba Bailian, Moonshot (mainland), Baidu Qianfan, Tencent Hunyuan and
  Volcano Engine Ark were written from memory; confirm each with a real key.
- Legal texts for Japan, Taiwan, Hong Kong, Singapore and the Partner Access Terms
  (v0.32.0): drafts that need review by a local lawyer in each place before a public
  launch there.
- First run with real keys (v0.31.0):
  - **Load my models** on each provider: which `/models` lists answer, and Anthropic's
    with a Bearer key;
  - the suggested model names, which change often (checked 2026-10-29);
  - Claude through Anthropic's OpenAI-compatible endpoint, with `temperature` left out;
  - GPT-5 with `max_completion_tokens`;
  - an existing install going through the setup and keeping its plans.
- Written answers with real handwriting and the owner's vision model (v0.29.0–v0.30.1):
  transcript faithfulness, photo warning thresholds, grades against a human correction;
  see [docs/validation-discursive.md](docs/validation-discursive.md).
- English prompts with the real model (v0.30.0): markers kept, lessons, handouts and
  quizzes in English.
- Vision requests with short reasoning (v0.30.1): a dense page read on the first try.
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
- [ ] Voices in Chinese and Japanese:
  - speech recognition with SenseVoice (zh, yue, ja) on the sherpa-onnx runtime;
  - Kokoro's Mandarin voices; check whether its Japanese voices run on sherpa-onnx;
  - Cantonese through a cloud voice;
  - turn end on `。？！`; per-language fillers (「えーと」, 「嗯」);
  - Traditional to Simplified before a Mandarin voice.
- [ ] Count characters instead of words for Chinese and Japanese written answers (字数,
      原稿用紙): unreadable threshold, rewrite detection, line limits.
- [ ] Whiteboard pictures from the Wikipedia of the app's language (today Portuguese).
- [ ] Code-side prompt snippets (`PromptLanguage::pick`) in Chinese and Japanese; today
      they fall back to English inside prompts that are otherwise translated.
- [ ] Native Anthropic Messages API client for Claude: Anthropic calls its
      OpenAI-compatible endpoint a testing aid, not a production path (`response_format`
      and `reasoning_effort` are ignored there; no prompt caching).
- [ ] Store the API key in the system keychain instead of the local database.
- [ ] Have a lawyer review `docs/legal` (LGPD; GDPR/UK GDPR; US state laws) before a
      public release.
- [x] Streaming AI client with silent whiteboard metadata and sentence callbacks.
- [x] Stream sentences into lesson playback before the full reply arrives (v0.17.0:
      first sentence at 2.7 s instead of 10.8 s in a live check).
- [x] Optional GPU (CUDA) build of candle; benchmark on supported hardware remains pending.
      With managed local AI (Phase 8), Whisper on the GPU shares VRAM with the model.
- [x] OCR fallback for scanned editais (vision model).
- [x] Linux/macOS native packaging scripts and CI; native execution remains unverified.
