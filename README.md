<p align="center">
  <img src="crates/ui/assets/logo.svg" width="88" alt="Know Giver logo">
</p>

# Know Giver

Desktop app for studying for Brazilian public exams (concursos).

Upload the exam notice (edital PDF) and the AI builds a study plan by week. Then study
with voice lessons (you speak, the tutor answers out loud), quizzes, and spaced reviews.

## Features

- **Edital → plan**: reads the PDF locally, sends only the syllabus to the AI, and builds
  a plan for your role, number of weeks and hours per week. Several editais can be kept.
- **Voice lessons**: local speech recognition (Parakeet or Whisper) with live captions,
  tutor via any OpenAI-compatible model, spoken answers with a Brazilian Portuguese Piper
  voice. The tutor starts
  speaking with the first sentence of its answer. Hands-free mode, or type.
- **Quizzes**: multiple-choice questions in the style of exam boards, with explanations.
- **Spaced review (SM-2)**: topics come back when you are about to forget them. The AI
  asks a recall question and grades your answer, or you grade yourself.
- **Exam board material**: upload past exams, answer keys and board handouts. The AI
  keeps what belongs to your edital, sets the rest aside with a reason, and uses real
  past questions in quizzes and as reference for lessons and handouts.
  Imported material can also be reused in another plan with independent relevance.
- **Scanned PDFs**: configure a vision model to transcribe pages that have no extractable
  text. The app explains that page images are sent to that configured service.
- **Daily study time**: Today shows active time, excluding inactivity and unfocused windows.
- **Voice controls**: preview, speed, pitch and avatar styles. Local Kokoro voices
  include a female Brazilian Portuguese voice. On Windows the runtime is launched
  with a UTF-8 code page so accented text is preserved. An optional speech service,
  off by default, can supply other voices.
- **Study handouts**: the AI writes each topic as a handout (explanation, key points,
  examples, exam pitfalls, summary, exercises with answers), exported as a PDF per topic,
  per subject or for the whole plan. Generation receives the original edital excerpt
  and board references; cached text is reused only while those sources remain unchanged.
- **Progress**: per subject, quiz accuracy, lessons taken, Markdown export.
- **Portuguese and English UI**, calm light/dark theme.

## Running

```bash
cp .env.example .env      # then fill in OPENAI_API_KEY
cargo run --release
```

`.env` overrides global environment variables with the same name. Everything in it can
also be set in the app's Settings. On first use, download the speech models in
Settings → Local models (Whisper base ≈ 290 MB, Piper ≈ 22 MB, a voice ≈ 63 MB).

Data (SQLite database and models) lives in `%LOCALAPPDATA%\KnowGiver`, or in
`KG_DATA_DIR` if set.

## Packaging (Windows)

```powershell
powershell -ExecutionPolicy Bypass -File scripts\package.ps1
```

Creates `dist\KnowGiver-<version>-windows-x64.zip`, plus an installer when
[Inno Setup 6](https://jrsoftware.org/isinfo.php) is installed.

## Structure

```
crates/
  app/      # know-giver binary (entry point, icon)
  ui/       # egui interface (screens, theme, translations)
  engine/   # app state, commands, background work, voice loop
  study/    # plans, reviews (SM-2), quizzes, SQLite storage
  ai/       # OpenAI-compatible client + tutor prompts (crates/ai/prompts)
  audio/    # mic capture, Parakeet/Whisper STT, Piper/Kokoro TTS, playback, model downloads
  pdf/      # edital text extraction
  handout/  # study handouts rendered to PDF with Typst
locales/    # UI translations (pt-BR, en)
```

Dependency direction: `app → ui → engine → {study, ai, audio, pdf}`. The UI never calls
the lower layers directly. See [docs/architecture.md](docs/architecture.md).

## Development

Rust 1.95 or newer is required by the current desktop dependencies.

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Manual checks against real services (need `.env` and network):

```bash
cargo run -p kg-ai --example live_check                         # every AI task
cargo run -p kg-engine --example e2e -- edital.pdf               # import → plan → quiz → review → handout
cargo run -p kg-engine --example board_check -- edital.pdf prova.pdf gabarito.pdf
cargo run --release -p kg-audio --example stt_check -- <models-dir> fala.wav
KG_SNAPSHOT_DIR=shots cargo test -p kg-ui snapshots -- --ignored # renders every screen to PNG
```

For the next local voice backend, see the [Kokoro evaluation](docs/kokoro-evaluation.md).
Piper remains the default. See [validation notes](docs/validation-v0.15.md) for remaining
voice and handout-quality limitations, and [native/CUDA builds](docs/builds.md) for
optional platform tooling.

## Credits

- Fonts: [Inter](https://rsms.me/inter/) and [IBM Plex Mono](https://github.com/IBM/plex),
  SIL Open Font License (`crates/ui/assets/fonts`).
- Teacher portraits (`crates/ui/assets/teachers`): "Lorelei" by Lisa Wischofsky,
  CC0 1.0, through [DiceBear](https://www.dicebear.com/styles/lorelei/); rebuilt with
  `python tools/teachers.py`.
- Dii voice (downloaded on demand): [OpenVoiceOS/pipertts_pt-BR_dii](https://huggingface.co/OpenVoiceOS/pipertts_pt-BR_dii),
  CC BY-NC-ND 4.0, personal non-commercial use.
- Whiteboard pictures are shown from [Wikipedia](https://pt.wikipedia.org); the
  caption names the article, where each picture's author and license are listed.
- Icons: [Lucide](https://lucide.dev), ISC license (`crates/ui/assets/icons`).

## References

- Architecture and style: [LightCraft](https://github.com/storytold/lightcraft)
- Transcription and audio: [Meetily](https://github.com/Zackriya-Solutions/meetily)
