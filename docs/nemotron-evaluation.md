# Nemotron evaluation

Status (2026-11-23): the default from 8 GB of memory (recommended: 16 GB and a dedicated
card with 4 GB), with CUDA on NVIDIA and Vulkan on AMD and Intel cards of 2 GB or more,
else the CPU; Parakeet stays the default below 8 GB (`crates/audio/src/hardware.rs`). Settings → Voice →
Speech recognition can still pick either.
See the ROADMAP section "Replace local transcription with Nemotron 3.5 0.6B".

## What runs

- **Runtime:** [NeMo-Speech.cpp](https://github.com/NVIDIA/NeMo-Speech.cpp) v0.3.0
  (Apache-2.0), official release archives. The app installs the CPU build and, where
  NVIDIA publishes one, the GPU build (Vulkan on Windows and Linux, Metal on Apple
  silicon) under `models/nemo-speech/{cpu,gpu}/`.
- **Model:** `nvidia/nemotron-3.5-asr-streaming-0.6b`, official `q8_0` GGUF (742 MB,
  OpenMDW-1.1), under `models/nemotron/`.
- **Server:** `nemo-speech serve --asr-model <gguf> --host 127.0.0.1 --port <free>
  --device vulkan|metal|cpu --threads 2 --no-ui --no-warmup`. The app waits for
  `GET /ready`, sends one second of faint noise as warmup, then posts each turn as a
  16 kHz mono PCM16 WAV to `POST /v1/audio/transcriptions` with `language=pt-BR`.
- **GPU or CPU:** order CUDA (NVIDIA, Windows and Linux x86_64; downloaded only when the
  NVIDIA driver is installed), Vulkan or Metal, then CPU. A GPU build runs when
  `nemo-speech doctor --json` lists a real GPU
  (not llvmpipe, SwiftShader or Microsoft Basic Render) with 1.5 GB free. If it crashes
  while loading, `know-giver-gpu-failed.txt` is left beside it and that build is skipped
  from then on (delete the file, or reinstall the runtime, to try again). Intel Macs
  have no GPU build. After installing, the app times every usable build on four seconds
  of audio and saves the fastest in `models/nemo-speech/fastest.txt` (a GPU build must
  beat the CPU by 20%); delete that file to time them again. Logs:
  `%TEMP%\know-giver-nemotron-{cuda,gpu,cpu}.log`.
- Everything is pinned with checksums in `local-ai.toml`.

## Languages

The model card lists 40 locales in three tiers. For the app's five languages:

| App language | Nemotron locale | Tier on the card |
|---|---|---|
| pt-BR | `pt-BR` | transcription-ready |
| en | `en-US` | transcription-ready |
| ja | `ja-JP` | transcription-ready |
| zh-Hans | `zh-CN` (Mandarin) | broad coverage |
| zh-Hant | none (no `zh-TW`) | — |

Voice lessons are Portuguese only today, so the app always sends `pt-BR`. Japanese and
Chinese lessons stay typed until they have voices; recognition alone does not make a
voice lesson. Traditional Chinese has no Nemotron locale at all.

## How to run the comparison

1. Record each phrase below as `NN.wav` (16 kHz or more, mono or stereo) and save what
   you said in `NN.txt` next to it. A lesson also leaves your last answer in
   `%LOCALAPPDATA%\KnowGiver\last-recording.wav`.
2. Run, from the repository, in PowerShell:

   ```
   cargo run --release -p kg-audio --example nemotron_check -- "$env:LOCALAPPDATA\KnowGiver\models" rec
   ```

   The second argument can be WAV files, a folder (every `.wav` in it) or `rec\*.wav`.
   In `cmd`, write the models folder as `"%LOCALAPPDATA%\KnowGiver\models"`. To force
   one build, set `$env:KG_NEMOTRON_BACKEND = "cpu"` (or `"gpu"`) before running.

   The first run downloads the runtime (about 31 MB on Windows) and the model (742 MB).
   It prints each transcript, the word error rate (WER) against the `.txt`, the time per
   clip, the real-time factor and the server's peak memory, then the same for Parakeet.
3. Run it again with `set KG_NEMOTRON_BACKEND=cpu` (and `=gpu`) to compare the builds.
4. Then try a real lesson with Nemotron chosen in Settings: live captions send the whole
   recording so far on every update, so check that they keep up.

## Phrases

Short answers (where Parakeet most often picks the wrong language):

1. Sim.
2. Não.
3. Isso.
4. Nada, nada.
5. Pode continuar.

Exam terms:

6. A crase indica a fusão da preposição a com o artigo feminino a.
7. O princípio da legalidade está no artigo trinta e sete da Constituição.
8. A Lei de Responsabilidade Fiscal limita os gastos com pessoal.
9. Habeas corpus protege a liberdade de locomoção.
10. A LOA, a LDO e o PPA são as leis orçamentárias.

Numbers and dates:

11. O prazo vence em vinte e três de novembro de dois mil e vinte e seis.
12. Foram mil duzentos e cinquenta candidatos para trinta vagas.
13. A alíquota passou de doze para quinze por cento.

Pauses and hesitation:

14. Eu acho que... a resposta é a letra C, porque o enunciado fala em... competência.
15. Hum, deixa eu pensar. É o poder de polícia?

Mixed technical words:

16. Não sei o que é machine learning.
17. O Python é uma linguagem interpretada.

## Results

| Machine (CPU, RAM, GPU) | Build | WER Nemotron | WER Parakeet | s per clip | Peak RAM | Notes |
|---|---|---|---|---|---|---|
| Ryzen 7 5700X, 32 GB, RTX 3060 | GPU (Vulkan) | — | — | — | — | With the server's warmup on, v0.3.0 aborts while loading (`GGML_ASSERT(ne1 == ne11)` in ggml-cpu); fixed with `--no-warmup` |
| Ryzen 7 5700X, 32 GB, RTX 3060 | CPU | 8.4% | 27.5% | 0.23 (RTF 0.09) | 1071 MB | 2026-11-23, Piper voice (synthetic). Parakeet without Whisper: "Sim/Não/Isso" came out in English; 17 of its 36 errors are digits ("37" for "trinta e sete"). Nemotron: "Python" → "O pai ganhou", "a com o artigo feminino a" lost the last "a" |

Second round, 2026-11-23, same machine, `--no-warmup` plus the app's own warmup. The 17
phrases in four Piper voices (Faber, Cadu, Jeff, Dii) and the Faber clips with noise and
60 Hz hum at about 15 dB SNR. WER per set: Faber / Cadu / Jeff / Dii / noisy.

| Build | WER Nemotron | Mean | RTF | Start | Peak RAM | "Sim / Não / Isso" wrong (of 15) |
|---|---|---|---|---|---|---|
| CUDA (RTX 3060, driver 621) | 8.4 / 13.7 / 20.6 / 10.7 / 10.7% | 12.8% | 0.01 | 1.0–1.8 s | ~630 MB | 4 (2 empty) |
| GPU (Vulkan, RTX 3060) | 9.9 / 11.5 / 18.3 / 13.7 / 16.8% | 14.0% | 0.12–0.14 | 1.3–1.8 s | ~250 MB | 6 (5 empty) |
| CPU | 8.4 / 13.7 / 22.1 / 9.9 / 10.7% | 13.0% | 0.08–0.10 | 0.8–1.3 s | ~770 MB | 4 (2 empty) |
| Parakeet (no Whisper) | 27.5 / 24.4 / 29.8 / 26.0 / 24.4% | 26.4% | 0.07–0.09 | ~4 s | — | most in English |

Vulkan output never has punctuation (the CPU build ends sentences with a period), so its
numbers differ from the CPU's beyond rounding. On this machine the CPU build is faster
and drops fewer one-word answers than Vulkan, which only saves RAM. CUDA gives the CPU's
transcripts (punctuation included) at about 25 ms per phrase.

Third round, 2026-11-23, Linux (Ubuntu 26.04, Mesa 26.0.8 RADV): Core i7-4770 (4th gen,
8 threads), 32 GB, AMD Radeon RX 5500 8 GB. Same sets, same request path (a Python
mirror of the app's resampling, `prepare_for_stt`, server flags and warm-up). `doctor`
lists only the RX 5500; the Intel HD 4600 and llvmpipe are not offered.

| Build | WER Nemotron | Mean | ms per clip | Start | Peak RAM | Short wrong (of 15) | Install timing (4 s) |
|---|---|---|---|---|---|---|---|
| Vulkan (RX 5500, RADV) | 10.7 / 13.0 / 19.1 / 13.7 / 16.8% | 14.7% | ~340 | 2.3 s | 153 MB | 6 (5 empty) | 395 ms |
| CPU (i7-4770) | 8.4 / 13.7 / 20.6 / 9.9 / 10.7% | 12.7% | ~560 | 0.7 s | 807 MB | 4 (2 empty) | 481 ms |

The Vulkan build loses the same one-word answers on AMD as on NVIDIA, so it is the
v0.3.0 Vulkan backend, not one driver. Vulkan is about 1.6x faster on real clips, but
the install timing (noise) shows only 1.2x, under the 20% lead it needs, so this
machine keeps the CPU build: 0.2 s slower per answer, fewer lost answers.

| | | | | | | |

## Acceptance (to decide with the numbers)

- Lower WER than Parakeet + Whisper on the phrases above, with no answer in another
  language.
- Delay after speech no worse than Parakeet's on an entry-level CPU (8 GB RAM, no
  dedicated GPU).
- Peak memory that leaves room for the app, the voice and the browser on 8 GB.
- On integrated GPUs, Vulkan must not slow down or starve the rest of the system.
