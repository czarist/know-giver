# Know Giver — Agent Instructions

Know Giver is a desktop application for studying for public exams (concursos).
The user uploads an exam notice (edital PDF), the AI generates a study plan and delivers interactive voice lessons (STT + LLM + TTS).

## References
- Architecture and style: inspired by LightCraft (pure Rust, egui, crate workspace)
- Audio and lessons: inspired by Meetily (local Whisper/Parakeet transcription, mic capture, voice responses)

## Stack
- **UI**: egui + eframe (native, no webview)
- **Language**: pure Rust
- **STT**: Whisper on candle (pure Rust) + cpal
- **TTS**: Piper binary as a child process (pt-BR voices), playback via rodio
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
- No hardcoded user-facing text: every string shown in the UI comes from `locales/<locale>.yml` via `t!("key")` (pt-BR is default and fallback). Add new keys to every locale file; `cargo test -p kg-ui` checks this. Error `Display` messages in lower crates are English, for logs only; the UI translates errors in `crates/ui/src/i18n.rs`
- Calm, readable UI focused on long study sessions

## Do not
- Unnecessary C/C++ dependencies
- unwrap/expect in production code
- Generic “AI slop” UI
