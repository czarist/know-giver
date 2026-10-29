//! Manual check: how long each Portuguese sentence takes with Kokoro kept
//! loaded in the app (the first one includes loading the model).
//!
//! cargo run --release -p kg-audio --example kokoro_speed -- <models-dir> [out.wav]

use std::time::Instant;

use kg_audio::{KokoroVoice, ModelStore, SpeechOptions, Synthesizer, Voice};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("kg_audio=info")
        .init();
    let mut args = std::env::args().skip(1);
    let store = ModelStore::new(
        args.next()
            .ok_or("usage: kokoro_speed <models-dir> [out.wav]")?,
    );
    let out = args.next();
    let tts = Synthesizer::local(
        &store,
        Voice::Kokoro(KokoroVoice::Dora),
        SpeechOptions::default(),
    )?;
    let t = Instant::now();
    tts.warm_up();
    println!("warm-up: {:?}", t.elapsed());
    let sentences = [
        "Olá! Hoje vamos estudar a crase.",
        "A crase é a fusão da preposição a com o artigo feminino a.",
        "Antes de palavra masculina, ela não acontece.",
    ];
    let mut last = None;
    for sentence in sentences {
        let t = Instant::now();
        let clip = tts.synthesize(sentence, &|| false)?;
        let audio = clip.samples.len() as f32 / clip.sample_rate as f32;
        println!(
            "{:>6.0} ms for {audio:.1} s: {sentence}",
            t.elapsed().as_millis()
        );
        last = Some(clip);
    }
    if let (Some(out), Some(clip)) = (out, last) {
        clip.write_wav(std::path::Path::new(&out))?;
    }
    Ok(())
}
