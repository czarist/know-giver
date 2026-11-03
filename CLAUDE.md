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
- **AI**: OpenAI-compatible client (Ollama / LiteLLM / GLM) and the GitHub Copilot SDK
- **Local OCR**: llama.cpp `llama-server` with PaddleOCR-VL (`local-ai.toml`)
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
- Prioritize Brazilian Portuguese (TTS, prompts, STT); Chinese and Japanese are typed-only until their voices exist
- No hardcoded user-facing text: every string shown in the UI comes from `locales/<locale>.yml` via `t!("key")` (pt-BR is default and fallback). Add new keys to every locale file; `cargo test -p kg-ui` checks this. Error `Display` messages in lower crates are English, for logs only; the UI translates errors in `crates/ui/src/i18n.rs`
- Calm, readable UI focused on long study sessions
- Prompts exist in Portuguese and English (`crates/ai/prompts/{pt,en}`) and follow the app's language. Change both together: same placeholders, JSON keys and line markers (`QUADRO:`, `ROTEIRO:`, `ETAPA:`, `FIM`, `[ilegível]`, `[?…?]`, `[rasura]`); a test in `crates/ai/src/prompts.rs` checks it. Text the code sends to the model also has both versions (`PromptLanguage::pick`)
- Images go only to the configured vision model, never to a text model as fallback; the model's answers about written work are checked in code (scores, quotes, pages and lines)
- Terms of use and privacy policy live in `docs/legal/` (pt-BR under the LGPD; en for US/UK/EU) and are compiled into the app (`crates/ui/src/legal.rs`). Change both languages together; any change to their content bumps the date in the texts and `TERMS_VERSION` (engine `settings.rs`), which asks every user to agree again. They must keep saying that the AI can be wrong and that quality depends on the chosen model
- Every feature covers all five languages (pt-BR, en, ja, zh-Hans, zh-Hant) in the same change, never Portuguese/English first and the rest later:
  - UI text: the key in every `locales/*.yml`;
  - AI prompts: the same file in every `crates/ai/prompts/<lang>/`, same placeholders, JSON keys and markers;
  - text the code sends to the model: Portuguese and English through `PromptLanguage::pick` (the others fall back to English);
  - legal texts: every language and region version in `docs/legal/`, with `TERMS_VERSION` bumped;
  - user-facing docs: every `README*.md` when the feature is visible there;
  - screenshots of new screens in each language when a README shows them.
  Run `cargo test -p kg-ui` and `-p kg-ai` (key and prompt parity). A real gap (for example, no voice yet for a language) is stated in the UI and in ROADMAP, never left silent
- Five UI and prompt languages: pt-BR, en, ja, zh-Hans, zh-Hant (`locales/*.yml`, `crates/ai/prompts/<lang>/`). New keys go into every locale file. Traditional Chinese can be converted from Simplified with OpenCC (`s2twp`) and then checked by hand
- Legal texts follow the region (`Region` in engine `settings.rs`), not the language: `docs/legal/{terms-of-use,privacy-policy}.<lang>.md` per region version, `partner-access.<lang>.md` for regions where the app is not public. The partner text names no country; a test checks it
- Teaching prompts follow the student's exam culture (`crates/ai/src/culture.rs`, from `Region::exam_culture`): exams, question formats, written-test conventions and the teacher's manner. A new region gets its note in Portuguese and English; extraction tasks never get it
- Free plans (`Provider::free_tier`) state what they do with the data; recheck them when providers change their terms
- First run is required (`Settings::needs_setup`, `SETUP_VERSION`): language, name, teacher, AI provider and key, terms. It never touches plans or progress. The provider list is `crates/ai/src/providers.rs`; model names there are suggestions, the app also lists the account's models
- Before committing: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; CI runs the same checks on every push to `dev` and before a release

## Git workflow
- `main` holds released code; `dev` collects finished work. The owner merges `dev` into `main` on GitHub.
- Start every task on a short-lived branch from `dev`: `git switch dev && git pull && git switch -c <type>/<short-name>` (types: `feat`, `fix`, `docs`, `chore`).
- Commit on that branch, then merge it into `dev` and push `dev`: `git switch dev && git merge --no-ff <branch> && git push origin dev`. Delete the task branch afterwards (`git branch -d <branch>`; also on `origin` if it was pushed).
- Never commit or push to `main` directly.
- A release still gets its CHANGELOG entry and version bump on the task branch. The annotated tag `vX.Y.Z` goes on `main` after the owner merges `dev` into `main`.
- Pushing the tag runs `.github/workflows/packages.yml`: it tests and packages on Windows, macOS and Linux, then publishes a GitHub release with fixed asset names (`KnowGiver-setup.exe`, `KnowGiver-windows-x64.zip`, `KnowGiver-linux-x86_64.tar.gz`, `KnowGiver-macos-arm64.tar.gz`, `SHA256SUMS.txt`). The README one-line installs (`scripts/install.ps1`, `scripts/install.sh`) and the in-app updater (engine `update.rs`) depend on those names; change them together. Binaries are never committed to the repository.

## External programs
The owner authorized these external projects, integrated as published and run outside the
app process, with all orchestration in Rust. This is not a general permission for other
non-Rust dependencies.

Shipping today:
- llama.cpp (<https://github.com/ggml-org/llama.cpp>): `llama-server` as a child process for
  local handwritten-answer OCR (engine `ocr.rs`), CPU builds everywhere and Vulkan on
  Windows and Linux, with CPU fallback. Its version and the OCR model (PaddleOCR-VL 1.5) are
  pinned with checksums in `local-ai.toml`, which the packaging scripts read and the app
  embeds; change them only there.
- GitHub Copilot CLI, bundled by the official `github-copilot-sdk` crate
  (<https://github.com/github/copilot-sdk>): sign-in and chat for the Copilot account
  provider (`crates/ai/src/copilot.rs`). The app never stores Copilot credentials.

Planned for managed local AI (ROADMAP Phase 8), not built; do not describe them as available
until they ship:
- LiteLLM (<https://github.com/BerriAI/litellm>): local proxy, embedded in the installer
- llama.cpp for text models, beyond OCR
- gpu-unlock (<https://github.com/Zagitta/gpu-unlock>, Linux) and Windows-GPU-UNLOCK
  (<https://github.com/impacifist/Windows-GPU-UNLOCK>): optional GPU preparation at boot, only
  on explicit user action through the administrator prompt

New pinned components go in `local-ai.toml`.

## Do not
- Unnecessary C/C++ dependencies
- unwrap/expect in production code
- Generic “AI slop” UI
