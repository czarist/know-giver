//! Manual check of Parakeet speech recognition through the local server.
//!
//! cargo run --release -p kg-audio --example parakeet_check -- <models-dir> <file.wav>...

use std::time::Instant;

use kg_audio::{AudioClip, ModelId, ModelStore, ParakeetStt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("kg_audio=info")
        .init();
    let mut args = std::env::args().skip(1);
    let store = ModelStore::new(
        args.next()
            .ok_or("usage: parakeet_check <models-dir> <wav>...")?,
    );
    for id in [ModelId::KokoroRuntime, ModelId::Parakeet] {
        if !store.is_installed(id) {
            return Err(format!("{id:?} is not installed").into());
        }
    }
    let exe = store.asr_server_exe().ok_or("speech server not found")?;
    let started = Instant::now();
    let mut stt = ParakeetStt::start(&exe, &store.parakeet_dir())?;
    println!("server ready in {:?}", started.elapsed());
    for path in args {
        let mut reader = hound::WavReader::open(&path)?;
        let spec = reader.spec();
        let samples: Vec<f32> = reader
            .samples::<i16>()
            .map(|s| s.map(|v| f32::from(v) / 32768.0))
            .collect::<Result<_, _>>()?;
        let clip = AudioClip {
            samples,
            sample_rate: spec.sample_rate,
        };
        let t = Instant::now();
        let text = stt.transcribe(&clip)?;
        println!("{path}: {text:?} ({:?})", t.elapsed());
    }
    Ok(())
}
