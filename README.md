<p align="center">
  <img src="crates/ui/assets/icon.png" width="128" height="128" alt="Know Giver icon: Thoth as a baboon under the crescent moon, engraved, on a jade tile">
</p>

<h1 align="center">Know Giver</h1>

<h3 align="center">Your exam. Your plan. A teacher who talks back.</h3>

<p align="center">
  <a href="README.md">English</a> · <a href="README.pt-BR.md">Português (Brasil)</a>
</p>

<p align="center">
  <b>A desktop app that builds study plans for exams, built in pure Rust.</b><br>
  Drop in the exam notice or syllabus PDF, get a week-by-week plan, and study out loud with a tutor that listens, explains and quizzes you.
</p>

<p align="center">
  <img alt="Pure Rust" src="https://img.shields.io/badge/pure-Rust-3f8f7c?style=flat-square&logo=rust&logoColor=white">
  <img alt="Windows, macOS, Linux" src="https://img.shields.io/badge/Windows%20%C2%B7%20macOS%20%C2%B7%20Linux-2c6b5c?style=flat-square">
  <img alt="Local-first voice" src="https://img.shields.io/badge/voice-local--first-2c6b5c?style=flat-square">
  <a href="#license"><img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-2c6b5c?style=flat-square"></a>
</p>

<br>

<p align="center">
  <img src="docs/images/lesson.png" alt="A voice lesson: the teacher's portrait with live captions, the whiteboard with key points, and the student's microphone tile" width="100%">
</p>

<p align="center">
  <a href="#from-syllabus-to-plan">Syllabus → plan</a> ·
  <a href="#voice-lessons">Voice lessons</a> ·
  <a href="#a-whiteboard-that-shows-not-just-tells">Whiteboard</a> ·
  <a href="#teachers--voices">Teachers</a> ·
  <a href="#read-and-highlight-your-handouts">Reader</a> ·
  <a href="#practice-review-handouts">Practice</a> ·
  <a href="#written-answers">Written answers</a> ·
  <a href="#private-by-default">Privacy</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#development">Development</a>
</p>

<br>

## From syllabus to plan

The exam notice is read on your machine; only its syllabus goes to the AI.

- **Pick your role.** Some exams cover several positions. Know Giver finds them and
  asks which one you are applying for, so the plan has the shared subjects plus only
  *your* position's specific ones.
- **Your schedule.** Number of weeks and hours per week shape a plan by week, subject
  and topic. Keep several exams side by side.
- **Scanned PDFs.** Pages without text can be transcribed by a vision model you configure.

<p align="center">
  <img src="docs/images/edital-roles.png" alt="Choosing the position from an exam notice that covers several" width="80%">
</p>

## Voice lessons

You speak, the tutor answers out loud, like a call with a patient teacher.

### 🎙️ Listening
Local speech recognition: **Parakeet** for live captions while you talk, **Whisper**
as a second opinion on short or doubtful answers. The microphone stays open for the
whole lesson, soft voices are brought up before recognition, and your turn ends after
a pause you choose in Settings, so you can stop to think of a word.

### 🗣️ Speaking
The tutor starts talking with the first sentence of its answer, says "Hum." while it
thinks, and stops when you interrupt. Captions follow its voice, sentence by sentence. Hands-free, push-to-talk (Space) or typing.

### 🧭 A lesson with a beginning and an end
Every lesson follows a plan built from the topic's handout (or written by the
teacher): each step brings the handout's key points, the teacher checks you got them
before moving on, and everything the quiz will ask is covered along the way. You
always see where you are ("Step 2 of 5"), and the lesson closes with a summary and an
offer to take the quiz. Pick **Quick**, **Normal** or **Full** lessons in Settings.
Taken lessons get a tick in your plan and keep their plan, board and closing words to
look back at. Leave a lesson halfway and **Continue lesson** picks it up later. English
words and sentences can be read in an English voice of their own.

### 🧠 Teaching
Any OpenAI-compatible model (Ollama, LiteLLM, GLM, …) behind tutor prompts written for
spoken conversation: short turns, one idea at a time, questions back to you. Prompts
follow the app's language: Portuguese by default, English when the app is in English.

## A whiteboard that shows, not just tells

Besides key points, the tutor writes **formulas**, **diagrams**, **comparison tables**,
**timelines**, **mind maps**, **"on the exam" tips**, **worked examples**, and puts up
**pictures** from Wikipedia (only the search term leaves your computer).

<p align="center">
  <img src="docs/images/whiteboard.png" alt="Whiteboard entries: formula, diagram, table, timeline, mind map and exam tip" width="80%">
</p>

## Teachers & voices

Choose who teaches: **Helena**, **Lia**, **Rafael** or **Otávio**.
Ink portraits that breathe, blink, nod while you talk and move their lips with the voice:
human enough to talk to, drawn enough to stay out of the uncanny valley.

Ten voices, half of them female, from **Piper** and **Kokoro**, all running locally,
plus four English voices for English passages (US and UK accents).

<p align="center">
  <img src="docs/images/teachers.png" alt="The four teachers in their states: resting, listening, thinking and speaking" width="70%">
</p>

## Read and highlight your handouts

Generated handouts open right inside the app, page by page, exactly as their PDF.
Drag over a passage to highlight it, click it to write a note, and find every
highlight in the side list. Jump through the contents, type a page, grab the page to
move it, or click the mouse wheel to scroll on its own. Know Giver remembers where you
stopped and how much of each handout you have read, and saves a PDF with your
highlights when you want one.

<p align="center">
  <img src="docs/images/reader.png" alt="A handout open in the reader, with a highlighted passage, its note in the side list and the reading progress bar" width="100%">
</p>

## Practice, review, handouts

- **Quizzes** in the style of the exam, with explanations; real past questions
  when you upload past exams.
- **Mistakes notebook**: every question you miss is kept until you get it right; it
  opens the next quiz on the topic and the teacher goes over it in the lesson.
- **Spaced review (SM-2)**: topics come back right before you would forget them, as
  three questions read aloud that you can answer by speaking.
- **Past exams and material**: past exams (with their answer key, in the same file or
  another one) and handouts, filtered against your
  syllabus, with the reason for whatever is set aside.
- **Handouts** per topic, subject or plan, with pictures, exercise cards and an
  answer key, saved as PDF.
- **Progress** per subject and **daily study time** that only counts active study.

## Written answers

When the exam notice has an essay or a discursive test, a **Written answers** area
appears. It never appears for a notice without one. A plan made before can have its
notice read again (**Re-read notice**) without losing topics, progress or handouts.

- **Prompts from your plan**: an argumentative essay, a technical discursive question or
  a case study, on topics you pick or on what you studied and missed. The criteria and
  points are shown before you write: the notice's own (with the passage quoted), yours,
  or Know Giver's generic ones, clearly labelled. The answer outline stays hidden until
  you hand in.
- **Write by hand, send photos**: JPEG or PNG pages, rotated, reordered, replaced or
  removed before sending. Dark, blurry or cut-off photos are flagged and you are asked
  for a new one instead of getting a grade.
- **Check the reading**: photo and transcript side by side. Your spelling and errors are
  kept; uncertain words are marked and never count against you. Fixing the reading is
  kept apart from the model's original; rewriting your answer becomes a new version.
- **Formative grade**: an estimated score per criterion and in total (summed by the app),
  with quotes that are checked against your text, strengths, problems with suggestions,
  three priorities and topics to review. Then the answer outline.
- **Progress**: scores by criterion across attempts graded on the same rubric, photo
  reading problems apart from writing and content ones, and recurring difficulties with
  examples from your own attempts.

Photos are read by the **vision model** set in Settings → AI (a model that takes images;
**Test image reading** checks it). One photo can hold one page or an open notebook with
two pages side by side. With a reasoning model, keep **Fast responses** on so a dense page
is not lost to long reasoning.

<table>
  <tr>
    <td><img src="docs/images/writing-check.png" alt="Checking the transcript: the photographed page beside the editable text, with uncertain words listed"></td>
    <td><img src="docs/images/writing.png" alt="A graded essay: criteria from the notice, estimated score per criterion with quotes, problems and suggestions"></td>
  </tr>
</table>

<table>
  <tr>
    <td><img src="docs/images/today.png" alt="Today: streak, reviews due and next step"></td>
    <td><img src="docs/images/board.png" alt="Exam board: past questions matched to the plan, with the ones out of scope set aside"></td>
  </tr>
</table>

## Private by default

- Speech recognition and the voices run on your machine. Recordings never leave it.
- Your data (SQLite database, models, log) lives in `%LOCALAPPDATA%\KnowGiver`, or in
  `KG_DATA_DIR` when set.
- Only text goes to the AI service you configure, and only what the task needs. The
  exception is opt-in: with a vision model set, photos of handwritten pages (and pages of
  scanned PDFs) are sent to it to be read. The app says where before sending; the photos
  stay in your data folder and are deleted with their attempt. Logs never hold images,
  answers or keys.

## Quick start

```bash
cp .env.example .env      # then fill in OPENAI_API_KEY (or set it in Settings)
cargo run --release
```

`.env` overrides global variables with the same name. On first use, download the speech
models in Settings. Rust 1.95 or newer.

**Windows package:**

```powershell
powershell -ExecutionPolicy Bypass -File scripts\package.ps1
```

Creates `dist\KnowGiver-<version>-windows-x64.zip`, plus an installer when
[Inno Setup 6](https://jrsoftware.org/isinfo.php) is installed.

## Development

```
crates/
  app/      # know-giver binary (entry point, icon)
  ui/       # egui interface (screens, theme, translations)
  engine/   # app state, commands, background work, voice loop
  study/    # plans, reviews (SM-2), quizzes, SQLite storage
  ai/       # OpenAI-compatible client + prompts (crates/ai/prompts/{pt,en})
  audio/    # mic capture, Parakeet/Whisper STT, Piper/Kokoro TTS, playback, downloads
  pdf/      # edital text extraction, roles
  handout/  # study handouts rendered to PDF with Typst
locales/    # UI translations (pt-BR, en)
tools/      # artwork generators (icon, teacher portraits)
```

Dependencies point one way: `app → ui → engine → {study, ai, audio, pdf, handout}`.
See [docs/architecture.md](docs/architecture.md).

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Building the whole workspace keeps every CPU core busy for minutes. On a machine that
overheats or has a weak power supply, limit the jobs: `cargo test -j 2 --workspace --
--test-threads=2` (or `jobs = 2` under `[build]` in `.cargo/config.toml`).

Manual checks against real services (need `.env` and network):

```bash
cargo run -p kg-ai --example live_check                          # every AI task (KG_PROMPT_LANG=en for English prompts)
cargo run -p kg-engine --example e2e -- edital.pdf                # import → plan → quiz → review → handout
cargo run --release -p kg-audio --example parakeet_check -- <models-dir> fala.wav
KG_SNAPSHOT_DIR=shots cargo test -p kg-ui snapshots -- --ignored  # renders every screen to PNG
```

More: [native/CUDA builds](docs/builds.md), [Kokoro evaluation](docs/kokoro-evaluation.md),
[validation notes](docs/validation-v0.15.md),
[written answers manual check](docs/validation-discursive.md).

## Credits

- App icon and cover art: Thoth as a baboon under the crescent moon, on a jade tile, by
  [Cezar Trentin](https://cezartrentin.dev) (`crates/ui/assets/art/thoth-tile.png`);
  `python tools/icon.py` builds the app and Windows icons from it.
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

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache License 2.0](LICENSE-APACHE), at your
option (`MIT OR Apache-2.0`). Unless you state otherwise, any contribution you submit for
inclusion is dual-licensed the same way, without additional terms.

Third-party fonts, portraits and voices keep their own licenses, listed under
[Credits](#credits). To report a vulnerability, see [SECURITY.md](SECURITY.md).
