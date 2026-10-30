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
- Terms of use and privacy policy live in `docs/legal/` and are compiled into the app. Change both languages together; a content change bumps the date in the texts and `TERMS_VERSION`, which asks users to agree again
- First run is required (language, name, teacher, AI provider and key, terms) and never touches plans or progress

## Git workflow
- `main` holds released code; `dev` collects finished work. The owner merges `dev` into `main` on GitHub.
- Start every task on a short-lived branch from `dev`: `git switch dev && git pull && git switch -c <type>/<short-name>` (types: `feat`, `fix`, `docs`, `chore`).
- Commit on that branch, then merge it into `dev` and push `dev`: `git switch dev && git merge --no-ff <branch> && git push origin dev`. Delete the task branch afterwards (`git branch -d <branch>`; also on `origin` if it was pushed).
- Never commit or push to `main` directly.
- A release still gets its CHANGELOG entry and version bump on the task branch. The annotated tag `vX.Y.Z` goes on `main` after the owner merges `dev` into `main`.

## Planned exceptions (not implemented)
Managed local AI (ROADMAP Phase 8) is planned, not built. The owner authorized these external
projects for it, integrated as published and run outside the app process, with all
orchestration in Rust:
- LiteLLM (<https://github.com/BerriAI/litellm>): local proxy, embedded in the installer
- llama.cpp (<https://github.com/ggml-org/llama.cpp>): inference engine, `llama-server` as a child process
- gpu-unlock (<https://github.com/Zagitta/gpu-unlock>, Linux) and Windows-GPU-UNLOCK
  (<https://github.com/impacifist/Windows-GPU-UNLOCK>): optional GPU preparation at boot, only
  on explicit user action through the administrator prompt

This is not a general permission for other non-Rust dependencies. Pinned versions and
checksums go in one manifest (`local-ai.toml`, planned). Do not describe these components as
available until they ship.

## Do not
- Unnecessary C/C++ dependencies
- unwrap/expect in production code
- Generic “AI slop” UI
