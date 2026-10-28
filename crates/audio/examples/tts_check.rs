//! Install a local voice and render a normalized sentence through the production path.
//! cargo run --release -p kg-audio --example tts_check -- <models-dir> <voice-id> <out.wav> [pitch]

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use kg_audio::{ModelStore, SpeechOptions, Synthesizer, Voice};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = args.next().ok_or("missing models directory")?;
    let id = args.next().ok_or("missing voice id")?;
    let output = args.next().ok_or("missing output WAV")?;
    let voice = Voice::ALL
        .into_iter()
        .find(|v| v.id() == id)
        .ok_or("unknown voice id")?;
    let pitch = args
        .next()
        .map(|p| p.parse::<f32>())
        .transpose()?
        .unwrap_or(0.0);
    let store = ModelStore::new(root);
    for model in [voice.runtime_id(), voice.model_id()] {
        if !store.is_installed(model) {
            let last = std::cell::Cell::new(0);
            store.download(
                model,
                &|done, total| {
                    let percent = done * 100 / total.max(1);
                    if percent >= last.get() + 10 {
                        println!("{model:?}: {percent}%");
                        last.set(percent);
                    }
                },
                &AtomicBool::new(false),
            )?;
        }
    }
    let sentence =
        "Bom dia! Hoje vamos ver o art. 5º, § 2º, da CF/88 e a Lei nº 8.112/90 (servidores).";
    let tts = Synthesizer::local(
        &store,
        voice,
        SpeechOptions {
            pitch,
            ..SpeechOptions::default()
        },
    )?;
    let started = Instant::now();
    let clip = tts.synthesize(&kg_audio::normalize_for_speech(sentence), &|| false)?;
    // Later sentences reuse the running voice.
    for text in ["Boa pergunta!", "Vamos ver mais um exemplo de crase."] {
        let t = Instant::now();
        let next = tts.synthesize(text, &|| false)?;
        println!(
            "{text:?}: {:.2}s audio in {:.2}s",
            next.duration_secs(),
            t.elapsed().as_secs_f64()
        );
    }
    println!(
        "{id}: {:.2}s audio, {} Hz, {:.2}s including startup",
        clip.duration_secs(),
        clip.sample_rate,
        started.elapsed().as_secs_f64()
    );
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: clip.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav = hound::WavWriter::create(output, spec)?;
    for value in clip.samples {
        wav.write_sample((value.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    wav.finalize()?;
    Ok(())
}
