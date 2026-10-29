//! Manual check of speech-to-text only: installs a Whisper model and
//! transcribes a 16-bit PCM WAV file.
//!
//! cargo run --release -p kg-audio --example stt_check -- <models-dir> <file.wav> [tiny|base|small]
//! Set `KG_STT_CONTEXT` to the text spoken just before, to test context prompting.

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use kg_audio::{AudioClip, ModelId, ModelStore, WhisperModel, WhisperStt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("kg_audio=info")
        .init();
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .ok_or("usage: stt_check <models-dir> <wav> [model]")?;
    let wav = args.next().ok_or("missing wav path")?;
    let model = match args.next().as_deref() {
        Some("tiny") => WhisperModel::Tiny,
        Some("small") => WhisperModel::Small,
        _ => WhisperModel::Base,
    };
    let store = ModelStore::new(root);
    let id = ModelId::Whisper(model);
    if !store.is_installed(id) {
        let t = Instant::now();
        let last = std::cell::Cell::new(0u64);
        store.download(
            id,
            &|done, total| {
                let pct = done * 100 / total.max(1);
                if pct >= last.get() + 20 {
                    last.set(pct);
                    println!("{id:?}: {pct}%");
                }
            },
            &AtomicBool::new(false),
        )?;
        println!("{id:?}: downloaded in {:?}", t.elapsed());
    }

    let clip = read_wav(&std::fs::read(&wav)?)?;
    println!(
        "audio: {:.1}s at {} Hz",
        clip.duration_secs(),
        clip.sample_rate
    );
    let t = Instant::now();
    let mut stt = WhisperStt::load(&store.whisper_dir(model))?;
    println!("{model:?} loaded in {:?}", t.elapsed());
    let t = Instant::now();
    // Optional context, like the tutor sentence before the answer.
    let context = std::env::var("KG_STT_CONTEXT").unwrap_or_default();
    let text = stt.transcribe_with_context(&clip, "pt", &context)?;
    println!("text: {text:?} ({:?})", t.elapsed());
    let t = Instant::now();
    let text = stt.transcribe_short_with_context(&clip, "pt", &context)?;
    println!("short window: {text:?} ({:?})", t.elapsed());
    Ok(())
}

/// Minimal reader for mono/stereo 16-bit PCM WAV.
fn read_wav(bytes: &[u8]) -> Result<AudioClip, Box<dyn std::error::Error>> {
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let (mut pos, mut channels, mut rate) = (12, 1u16, 16_000u32);
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32_at(pos + 4) as usize;
        let body = pos + 8;
        if id == b"fmt " {
            channels = u16_at(body + 2);
            rate = u32_at(body + 4);
        } else if id == b"data" {
            let data = &bytes[body..(body + len).min(bytes.len())];
            let samples: Vec<f32> = data
                .chunks_exact(2 * usize::from(channels))
                .map(|frame| f32::from(i16::from_le_bytes([frame[0], frame[1]])) / 32768.0)
                .collect();
            return Ok(AudioClip {
                samples,
                sample_rate: rate,
            });
        }
        pos = body + len + (len % 2);
    }
    Err("no data chunk".into())
}
