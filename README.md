# Know Giver

Desktop application for studying for public exams (concursos).

Upload an exam notice (edital PDF), the AI generates a personalized study plan and delivers interactive lessons with voice (you speak, the app answers in audio).

## Idea

- Upload edital → content extraction
- Generate study plan + activities
- Lessons in the style of a human tutor (with speech and listening)
- Progress tracking, review and quizzes

## Stack

- **UI**: egui + eframe (pure Rust, native)
- **Audio**: microphone capture + STT (Whisper/Parakeet) + TTS (Piper pt-BR)
- **AI**: OpenAI-compatible client (Ollama / LiteLLM / GLM)
- **PDF**: local extraction
- **Storage**: SQLite

## How to run (once code exists)

```bash
cargo run --release -p know-giver
```

## Planned structure

```
crates/
  engine/      # main logic
  audio/       # STT + TTS
  study/       # plans, progress, quizzes
  ui/          # egui interface
  ai/          # LLM client
```

## References

- Architecture and style: [LightCraft](https://github.com/storytold/lightcraft)
- Transcription and audio: [Meetily](https://github.com/Zackriya-Solutions/meetily)
