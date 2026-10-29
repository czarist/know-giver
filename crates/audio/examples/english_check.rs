//! Manual check: a Portuguese sentence with an English passage, read by the
//! main voice and an English Kokoro voice, saved as one WAV.
//!
//! cargo run --release -p kg-audio --example english_check -- <models-dir> <out.wav>

use std::time::Instant;

use kg_audio::{EnglishVoice, KokoroVoice, ModelStore, SpeechOptions, Synthesizer, Voice};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .ok_or("usage: english_check <models-dir> <out.wav>")?;
    let out = args.next().ok_or("missing output path")?;
    let store = ModelStore::new(root);
    let options = SpeechOptions::default();
    let tts = Synthesizer::local(&store, Voice::Kokoro(KokoroVoice::Dora), options)?.with_english(
        &store,
        EnglishVoice::Michael,
        options,
    );
    println!("English voice: {}", tts.speaks_english());
    let text =
        "A frase [en]I have been working here since 2020.[/en] usa o present perfect continuous.";
    for (lang, piece) in tts.pieces(text) {
        println!("{lang:?}: {piece}");
    }
    let t = Instant::now();
    let clip = tts.synthesize_marked(text, &|| false)?;
    println!(
        "{:.1} s of audio in {:?}",
        clip.samples.len() as f32 / clip.sample_rate as f32,
        t.elapsed()
    );
    clip.write_wav(std::path::Path::new(&out))?;
    println!("saved {out}");
    Ok(())
}
