//! Manual end-to-end check: installs models, speaks a sentence with Piper and
//! transcribes it back with Whisper.
//!
//! cargo run --release -p kg-audio --example voice_check -- <models-dir> [tiny|base|small] [out.wav]

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
    let sentence =
        "Bom dia! Hoje vamos ver o art. 5º, § 2º, da CF/88 e a Lei nº 8.112/90 (servidores).";
    let t = Instant::now();
    let mut clip = kg_audio::AudioClip {
        samples: Vec::new(),
        sample_rate: 22_050,
    };
    for chunk in kg_audio::split_for_speech(sentence) {
        println!("speaking: {chunk:?}");
        let part = tts.synthesize(&chunk)?;
        clip.sample_rate = part.sample_rate;
        clip.samples.extend(part.samples);
    }
    println!(
        "piper: {:.1}s of audio at {} Hz in {:?}",
        clip.duration_secs(),
        clip.sample_rate,
        t.elapsed()
    );
    if let Some(path) = args.next() {
        write_wav(&path, &clip)?;
        println!("saved {path}");
    }

    let t = Instant::now();
    let mut stt = WhisperStt::load(&store.whisper_dir(whisper))?;
    println!("whisper {whisper:?} loaded in {:?}", t.elapsed());
    let t = Instant::now();
    let text = stt.transcribe(&clip, "pt")?;
    println!("whisper: {:?} in {:?}", text, t.elapsed());
    println!("expected: {sentence:?}");
    Ok(())
}

/// 16-bit mono PCM WAV.
fn write_wav(path: &str, clip: &kg_audio::AudioClip) -> std::io::Result<()> {
    let data: Vec<u8> = clip
        .samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect();
    let mut out = Vec::with_capacity(44 + data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&clip.sample_rate.to_le_bytes());
    out.extend_from_slice(&(clip.sample_rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    std::fs::write(path, out)
}
