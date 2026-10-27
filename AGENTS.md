# Know Giver — Agent Instructions

Know Giver is a desktop application for studying for public exams (concursos).
The user uploads an exam notice (edital PDF), the AI generates a study plan and delivers interactive voice lessons (STT + LLM + TTS).

## References
- Architecture and style: inspired by LightCraft (pure Rust, egui, crate workspace)
- Audio and lessons: inspired by Meetily (local Whisper/Parakeet transcription, mic capture, voice responses)

## Stack
- **UI**: egui + eframe (native, no webview)
- **Language**: pure Rust
- **STT**: whisper-rs or Parakeet (via ONNX) + cpal
- **TTS**: piper-rs (priority Brazilian Portuguese) or fallback
- **PDF**: pdf-extract / oxidize-pdf
- **AI**: OpenAI-compatible client (Ollama / LiteLLM / GLM)
- **Storage**: SQLite

## Organization
- Cargo workspace with separate crates
- Clear layers (engine / ui / audio / study / ai)
- No unwrap/expect in production code
- Prefer local-first

## Main modules
- `pdf/` — edital extraction
- `study/` — study plans, progress, quizzes
- `audio/` — capture, STT, TTS
- `ai/` — LLM client + tutor prompts
- `ui/` — egui interface

## Rules
- Do not mix business logic into the UI
- Errors with context (thiserror)
- Prioritize Brazilian Portuguese (TTS, prompts, STT)
- Calm, readable UI focused on long study sessions

## Do not
- Unnecessary C/C++ dependencies
- unwrap/expect in production code
- Generic “AI slop” UI
