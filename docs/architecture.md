# Architecture — Know Giver

## Overview

Know Giver is a pure Rust desktop app (egui) focused on studying for public exams.
The main flow is:

1. User uploads an exam notice (edital PDF)
2. System extracts the text and sends it to the AI
3. AI generates a structured study plan
4. User studies following the plan
5. In lessons, the user speaks and the app answers with voice

## Layers

```
┌─────────────────────────────────────┐
│              UI (egui)              │
│  plans, progress, lesson, settings  │
└─────────────────┬───────────────────┘
                  │
┌─────────────────▼───────────────────┐
│             Engine                  │
│  orchestration, state, commands     │
└──────┬──────────┬──────────┬────────┘
       │          │          │
┌──────▼──┐ ┌─────▼────┐ ┌───▼────┐
│  Study  │ │   AI     │ │ Audio  │
│ plans   │ │ client   │ │ STT/TTS│
│ quizzes │ │ prompts  │ │ mic    │
└─────────┘ └──────────┘ └────────┘
```

## Modules

### Study
- Represents the study plan (subjects, topics, schedule)
- User progress
- Quizzes and activities
- Persistence in SQLite

### AI
- OpenAI-compatible client
- Tutor prompts (see `docs/tutor-prompts.md`)
- Study plan generation from the edital
- Answers during lessons

### Audio
- Microphone capture (cpal)
- Local STT (Whisper or Parakeet)
- Local TTS (Piper with pt-BR voice)
- Pipeline: audio → text → LLM → audio back

### UI
- egui interface
- Screens: upload, plan, lesson, progress, settings
- Visual feedback for listening and speaking

## Principles

- Local-first when possible
- Clear separation between UI and logic
- Errors handled with context
- Priority for Brazilian Portuguese

## Decisions

- **Scanned documents**: configuring `MODEL_VISION` enables OCR only when local text
  extraction finds no text. Hayro rasterizes bounded page images locally; the configured
  vision endpoint transcribes them. UI copy explains the image upload.
- **Study time**: the UI reports focus/interaction while the engine accounts elapsed
  time, skips suspend gaps and persists daily totals in SQLite migration 5.
- **Voice backends**: Piper remains the local default. Optional HTTP speech sends text
  only after the user selects it. Kokoro uses a cancellable child process; its pinned
  Windows runtime is disabled after a real UTF-8 compatibility failure.
- **Streaming**: the AI client supports SSE and sentence callbacks. The lesson engine
  still uses complete replies; streaming playback remains follow-up work.

- **Speech recognition**: Whisper runs on [candle](https://github.com/huggingface/candle)
  (pure Rust, CPU), with weights downloaded from Hugging Face. `whisper-rs` would need
  clang and cmake to build whisper.cpp. Whisper base is the default: about 3 s for 10 s
  of speech, and small is more accurate but about 3× slower.
- **Speech synthesis**: the official Piper binary runs as a child process with a pt-BR
  voice. No C/C++ is compiled into the app. Answers are spoken sentence by sentence, so
  playback starts early.
- **AI**: one OpenAI-compatible client, with a model per task (`MODEL_*` in `.env`) and a
  fallback model. `reasoning_effort: "low"` keeps GLM 5.x fast. Without it, plan generation
  on a full edital spent the whole token budget reasoning.
- **Edital excerpt**: only the opening of the edital and its syllabus section go into the
  prompt. For the TJ-CE edital, that cut 111k characters down to 25k.
- **Background work**: slow tasks run on worker threads and send events back to the engine.
  `Engine::poll` applies them on the UI thread, and results from closed sessions are dropped.
- **Quizzes**: options are shuffled after generation, because models tend to put the right
  answer first.
- **Translations**: every UI string comes from `locales/*.yml`, and tests check that both
  languages have the same keys. Errors in lower crates are English and only go to logs.
  The UI translates them for the user.

