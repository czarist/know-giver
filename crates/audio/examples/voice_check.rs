//! Manual end-to-end check: installs models, speaks a sentence with Piper and
//! transcribes it back with Whisper.
//!
//! cargo run --release -p kg-audio --example voice_check -- <models-dir> [tiny|base|small]

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use kg_audio::{ModelId, ModelStore, PiperTts, PiperVoice, WhisperModel, WhisperStt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("kg_audio=info")
        .init();
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .ok_or("usage: voice_check <models-dir> [model]")?;
    let whisper = match args.next().as_deref() {
        Some("tiny") => WhisperModel::Tiny,
        Some("small") => WhisperModel::Small,
        _ => WhisperModel::Base,
    };
    let store = ModelStore::new(root);
    let cancel = AtomicBool::new(false);

    for id in [
        ModelId::PiperRuntime,
        ModelId::Voice(PiperVoice::Faber),
        ModelId::Whisper(whisper),
    ] {
        if store.is_installed(id) {
            println!("{id:?}: installed");
            continue;
        }
        let t = Instant::now();
        let last = std::cell::Cell::new(0u64);
        store.download(
            id,
            &|done, total| {
                let pct = done * 100 / total.max(1);
                if pct >= last.get() + 10 {
                    last.set(pct);
                    println!("{id:?}: {pct}%");
                }
            },
            &cancel,
        )?;
        println!("{id:?}: downloaded in {:?}", t.elapsed());
    }

    let exe = store.piper_exe().ok_or("piper missing")?;
    let (onnx, json) = store.voice_files(PiperVoice::Faber);
    let tts = PiperTts::new(exe, onnx, &json, 1.0)?;
    let sentence = "Bom dia! Hoje vamos estudar o emprego da crase antes de horas, um tema muito cobrado em concursos.";
    let t = Instant::now();
    let clip = tts.synthesize(sentence)?;
    println!(
        "piper: {:.1}s of audio at {} Hz in {:?}",
        clip.duration_secs(),
        clip.sample_rate,
        t.elapsed()
    );

    let t = Instant::now();
    let mut stt = WhisperStt::load(&store.whisper_dir(whisper))?;
    println!("whisper {whisper:?} loaded in {:?}", t.elapsed());
    let t = Instant::now();
    let text = stt.transcribe(&clip, "pt")?;
    println!("whisper: {:?} in {:?}", text, t.elapsed());
    println!("expected: {sentence:?}");
    Ok(())
}
