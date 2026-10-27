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
