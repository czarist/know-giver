# Version 0.15 validation

Production functionality and unverified acceptance checks are tracked separately in
ROADMAP.md. Work was closed at the user's request to control cost.

v0.15.2 prepares a sibling of the pinned Windows runtime with `activeCodePage` set to
UTF-8. A local run of "união" reached sherpa as the UTF-8 bytes `C3 A3` and wrote a
WAV. Subjective listening acceptance is still open.

## Real services and binaries

- Two handout requests used the configured AI endpoint with synthetic multi-role and
  truncated edital excerpts. Both stayed on Crase and excluded engineering content.
  Review nevertheless found factual/exercise errors (including an incorrect answer
  about hours). v0.16.0 adds an automatic review and blind-solve pass (see CHANGELOG); it reduces but does not eliminate such errors. Local review artifacts are
  under `target/handout-scope-review-v015/`; they are not committed.
- Official sherpa-onnx v1.12.20 with Kokoro v1.0 produced WAVs on Windows when the input
  had lost accents, but failed with the actual UTF-8 Portuguese corpus (`0xC0000409`,
  non-UTF-8 string reported). Those WAVs are not Portuguese acceptance evidence.
  v0.15.2 keeps that runtime and launches a UTF-8 manifest copy instead of leaving
  Kokoro disabled.
- Piper synthesized the accented corpus successfully: 11.87 seconds of audio in
  1.80 seconds. This establishes synthesis, not a subjective listening preference.
- The official signed Inno Setup 6.7.3 compiler was installed under
  `target/build-tools/InnoSetup6` for installer verification. No final installer build,
  installation test or distribution was performed before closing work.

## Scope of automated checks

Final CPU suite: 122 tests passed; the four GPU snapshot tests also passed when run
explicitly. Clippy passed with warnings denied, and Rustdoc completed without warnings.
Light/dark components and Portuguese voice settings screenshots were inspected.

Tests cover source mapping, source-aware caching, OCR rasterization bounds, daily time
and idle/suspend handling, speech HTTP contract, child-process cancellation, translated
UI keys, and byte-fragmented SSE including metadata suppression. A local HTTP test is
not a live cloud-provider test. Default CPU tests do not validate the CUDA toolchain.

Native package scripts are syntax-checked on this Windows machine. GitHub workflow
execution, Linux/macOS desktop behavior, physical microphone use and subjective voice
quality require additional validation. The streaming client is available, but the
lesson engine still waits for the full tutor reply.
