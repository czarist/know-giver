# Know Giver — Agent Instructions

Know Giver is a desktop application for studying for exams (public exams, entrance exams, certifications).
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
- No unwrap/expect in production
- Prefer local-first

## Main modules
- `pdf/` — edital extraction
- `study/` — study plans, progress, quizzes, written answers (`discursive`)
- `audio/` — capture, STT, TTS
- `ai/` — LLM client + prompts in `crates/ai/prompts/pt/` and `crates/ai/prompts/en/`
- `engine/` — commands, background work; written answers in `engine/src/discursive/`
- `ui/` — egui interface

## Rules
- Do not mix business logic into the UI
- Errors with context (thiserror)
- Prioritize Brazilian Portuguese (TTS, prompts, STT)
- No hardcoded user-facing text: every string shown in the UI comes from `locales/<locale>.yml` via `t!("key")` (pt-BR is default and fallback). Add new keys to every locale file; `cargo test -p kg-ui` checks this. Error `Display` messages in lower crates are English, for logs only; the UI translates errors in `crates/ui/src/i18n.rs`
- Calm, readable UI focused on long study sessions
- Prompts exist in Portuguese and English (`crates/ai/prompts/{pt,en}`) and follow the app's language. Change both together: same placeholders, JSON keys and line markers (`QUADRO:`, `ROTEIRO:`, `ETAPA:`, `FIM`, `[ilegível]`, `[?…?]`, `[rasura]`); a test in `crates/ai/src/prompts.rs` checks it. Text the code sends to the model also has both versions (`PromptLanguage::pick`)
- Images go only to the configured vision model, never to a text model as fallback; the model's answers about written work are checked in code (scores, quotes, pages and lines)
- The owner's PC shuts down under long full-CPU builds: run cargo with `-j 2` to `-j 4` and `--test-threads` to match

## Do not
- Unnecessary C/C++ dependencies
- unwrap/expect in production code
- Generic “AI slop” UI
