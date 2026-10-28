# Kokoro evaluation

Status in v0.15: backend, model management and preview are implemented, but local
Kokoro is disabled on Windows because the pinned executable fails on accented UTF-8
arguments. Piper remains the default. See [validation notes](validation-v0.15.md).

## Reproducible candidate

Use the **sherpa-onnx v1.12.20** prebuilt CPU executable and the unquantized
**kokoro-multi-lang-v1_0** bundle (53 speakers). Keep the executable with its bundled
runtime files. Do not substitute v1.1: speaker IDs differ between bundles.

- [Runtime release](https://github.com/k2-fsa/sherpa-onnx/releases/tag/v1.12.20)
- [Model bundle and speaker mapping](https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/kokoro.html)
- [Pinned language and model options](https://github.com/k2-fsa/sherpa-onnx/blob/v1.12.20/sherpa-onnx/csrc/offline-tts-kokoro-model-config.cc)
- [Pinned executable interface](https://github.com/k2-fsa/sherpa-onnx/blob/v1.12.20/sherpa-onnx/csrc/sherpa-onnx-offline-tts.cc)

The model directory must contain `model.onnx`, `voices.bin`, `tokens.txt` and
`espeak-ng-data/`. The harness uses `pt-br`, two CPU threads, speech rate 0.95,
and speakers 42 (Dora), 43 (Alex), 44 (Santa). It uses the app's Portuguese text
normalizer. Files are supplied locally; the harness does not download or install them.

```powershell
cargo run --release -p kg-audio --example kokoro_check -- C:\tts\bin\sherpa-onnx-offline-tts.exe C:\tts\kokoro-multi-lang-v1_0 C:\tts\listening-results
```

The output directory must not exist and its parent must exist. Each sample produces
WAV, normalized text and process logs. Console timings include process/model startup;
the executable's stderr includes synthesis timing and real-time factor. A successful
exit and nonempty file do not establish audio quality. Inspect and listen to the WAVs.

## Acceptance before app integration

1. Record runtime archive name, model archive name, CPU, sample rate, timings and RAM.
2. Listen to all nine samples: legal abbreviations, pauses, accents, currency and
   questions must remain understandable; reject silence, clipping or broken phonemes.
3. Compare the first legal sentence against `voice_check` with Piper Faber. Compare
   further real tutor replies before choosing a default; record subjective preferences.
4. Check cold startup latency per sentence, interruption and cleanup on Windows.
5. Only then add atomic model installation, installed-voice selection, preview and
   matching avatar. Keep Piper available and preserve old saved settings.

The upstream overview focuses on English/Chinese examples; the pinned source exposes
`pt-br`. That supports testing the candidate, not a claim that Portuguese quality is
already verified. Newer runtime releases need a fresh compatibility check.
