# Native and accelerated builds

The default build is CPU-only. Build on each target OS rather than copying a binary
between operating systems. `scripts/package.ps1` builds Windows portable/installer
packages; `bash scripts/package-unix.sh` builds Linux tarballs and macOS app bundles.
`.github/workflows/packages.yml` runs native builds for version tags or a manual run.
It uploads workflow artifacts; it does not publish a GitHub release or sign packages.

Linux builds need the platform audio/window development libraries, for example
`libasound2-dev`, `libudev-dev`, `libxkbcommon-dev`, `libwayland-dev` and
`libegl1-mesa-dev` on Ubuntu. macOS bundles include the microphone usage declaration.
macOS signing/notarization and checks on actual Linux/macOS desktops require those
platforms and are not established by a successful Windows build.

## Optional CUDA Whisper

With NVIDIA's CUDA toolkit (`nvcc`) and the platform C++ build tools installed:

```text
cargo build --release -p know-giver --features cuda
cargo run --release -p kg-audio --features cuda --example stt_check -- <models-dir> <speech.wav> small
```

The feature propagates through app → ui → engine → audio into Candle's CUDA backend.
Whisper selects device 0; if device initialization fails it logs the reason and uses
CPU. Set `KG_WHISPER_DEVICE=cpu` to force CPU in the same build for comparison.
Models remain local and their format is unchanged.

CUDA is opt-in and requires a native CUDA build toolchain even for `cargo check`.
Run normal workspace checks without `--all-features` on CPU-only machines; on a CUDA
builder, also check `--features cuda`. Record transcription agreement, model load
time and decode latency against CPU before claiming a speedup.

Reference: [Candle](https://github.com/huggingface/candle).
