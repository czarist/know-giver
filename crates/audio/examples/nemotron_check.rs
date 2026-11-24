//! Manual comparison of Nemotron 3.5 (NeMo-Speech.cpp, pt-BR) and Parakeet
//! on the same recordings. Downloads the pinned NeMo-Speech runtime and the
//! Nemotron model into `<models-dir>` when they are missing.
//!
//! cargo run --release -p kg-audio --example nemotron_check -- <models-dir> <file.wav | folder | folder\*.wav>...
//!
//! - A `file.txt` next to `file.wav` holds what was really said; the check
//!   then prints each recognizer's word error rate (WER).
//! - `KG_NEMOTRON_BACKEND=cpu` or `=gpu` runs only that build (GPU first,
//!   then CPU, by default).
//! - On Windows the app's models live in `%LOCALAPPDATA%\KnowGiver\models`.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use kg_audio::{AudioClip, ModelId, ModelStore, NemotronStt, ParakeetStt};

struct Sample {
    path: String,
    clip: AudioClip,
    reference: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("kg_audio=info")
        .init();
    let mut args = std::env::args().skip(1);
    let store = ModelStore::new(expand_env(
        &args
            .next()
            .ok_or("usage: nemotron_check <models-dir> <wav>...")?,
    ));
    let mut paths = Vec::new();
    for arg in args {
        paths.extend(expand(&expand_env(&arg))?);
    }
    if paths.is_empty() {
        return Err("no .wav files given (pass files, a folder or folder\\*.wav)".into());
    }
    let samples = paths
        .into_iter()
        .map(|path| {
            let clip = read_wav(&path).map_err(|e| format!("{path}: {e}"))?;
            let reference = std::fs::read_to_string(Path::new(&path).with_extension("txt")).ok();
            Ok(Sample {
                path,
                clip,
                reference,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    download_missing(&store)?;

    let hardware = kg_audio::SpeechHardware::detect();
    println!(
        "hardware: {} MB RAM, dedicated GPU {:?}, default recognizer {:?}",
        hardware.ram_bytes.unwrap_or(0) / (1024 * 1024),
        hardware.dedicated_gpu,
        hardware.recommended_stt()
    );
    let timed = Instant::now();
    match store.choose_nemotron_backend() {
        Some(fastest) => println!(
            "build chosen here (dedicated card, else fastest): {} ({:?})",
            fastest.as_str(),
            timed.elapsed()
        ),
        None => println!("no nemotron build could be timed"),
    }
    let runtimes = store.nemo_speech_runtimes();
    let model = store.nemotron_model().ok_or("nemotron model not found")?;
    let started = Instant::now();
    let mut nemotron = NemotronStt::start(&runtimes, &model)?;
    println!(
        "nemotron ready on {} in {:?} (pid {})",
        nemotron.backend().as_str(),
        started.elapsed(),
        nemotron.pid()
    );
    let mut nemotron_score = Score::default();
    for sample in &samples {
        let t = Instant::now();
        let text = nemotron.transcribe(&sample.clip)?;
        let elapsed = t.elapsed();
        report("nemotron", sample, &text, elapsed, &mut nemotron_score);
    }
    if let Some(memory) = peak_memory(nemotron.pid()) {
        println!("nemotron server memory: {memory}");
    }
    drop(nemotron);

    let mut parakeet_score = Score::default();
    if store.is_installed(ModelId::Parakeet) && store.is_installed(ModelId::KokoroRuntime) {
        let exe = store.asr_server_exe().ok_or("speech server not found")?;
        let mut parakeet = ParakeetStt::start(&exe, &store.parakeet_dir())?;
        for sample in &samples {
            let t = Instant::now();
            let text = parakeet.transcribe(&sample.clip)?;
            let elapsed = t.elapsed();
            report("parakeet", sample, &text, elapsed, &mut parakeet_score);
        }
    } else {
        println!("parakeet is not installed; skipping the comparison");
    }

    println!();
    for (name, score) in [("nemotron", &nemotron_score), ("parakeet", &parakeet_score)] {
        if score.clips > 0 {
            println!(
                "{name}: {} clips, {:.1} s audio, {:?} total, real-time factor {:.2}{}",
                score.clips,
                score.audio_secs,
                score.elapsed,
                score.elapsed.as_secs_f32() / score.audio_secs.max(0.01),
                score.wer().map_or(String::new(), |wer| format!(
                    ", WER {:.1}% over {} reference words",
                    wer * 100.0,
                    score.words
                )),
            );
        }
    }
    Ok(())
}

fn download_missing(store: &ModelStore) -> Result<(), Box<dyn std::error::Error>> {
    let cancel = AtomicBool::new(false);
    for id in [ModelId::NemoSpeechRuntime, ModelId::Nemotron] {
        if store.is_installed(id) {
            continue;
        }
        println!("downloading {id:?}…");
        let last = std::cell::Cell::new(0);
        store.download(
            id,
            &|done, total| {
                let percent = done * 100 / total.max(1);
                if percent >= last.get() + 10 {
                    last.set(percent);
                    println!("  {percent}%");
                }
            },
            &cancel,
        )?;
    }
    Ok(())
}

#[derive(Default)]
struct Score {
    clips: usize,
    audio_secs: f32,
    elapsed: Duration,
    errors: usize,
    words: usize,
}

impl Score {
    fn wer(&self) -> Option<f32> {
        (self.words > 0).then(|| self.errors as f32 / self.words as f32)
    }
}

fn report(name: &str, sample: &Sample, text: &str, elapsed: Duration, score: &mut Score) {
    score.clips += 1;
    score.audio_secs += sample.clip.duration_secs();
    score.elapsed += elapsed;
    let mut line = format!(
        "{name:<9} {} ({:.1} s): {text:?} in {elapsed:?}",
        sample.path,
        sample.clip.duration_secs()
    );
    if let Some(reference) = &sample.reference {
        let expected = words(reference);
        let errors = edit_distance(&expected, &words(text));
        score.errors += errors;
        score.words += expected.len();
        line.push_str(&format!(" — {errors} errors / {} words", expected.len()));
    }
    println!("{line}");
}

/// Lowercase words without accents or punctuation, so only real
/// recognition errors count.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            w.to_lowercase()
                .chars()
                .map(|c| match c {
                    'á' | 'à' | 'â' | 'ã' => 'a',
                    'é' | 'ê' => 'e',
                    'í' => 'i',
                    'ó' | 'ô' | 'õ' => 'o',
                    'ú' | 'ü' => 'u',
                    'ç' => 'c',
                    c => c,
                })
                .collect()
        })
        .collect()
}

/// Word-level Levenshtein distance: substitutions, insertions, deletions.
fn edit_distance(a: &[String], b: &[String]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, wa) in a.iter().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, wb) in b.iter().enumerate() {
            let current = row[j + 1];
            row[j + 1] = if wa == wb {
                previous
            } else {
                1 + previous.min(row[j]).min(current)
            };
            previous = current;
        }
    }
    row[b.len()]
}

/// Peak (Windows) or current (elsewhere) resident memory of a process.
fn peak_memory(pid: u32) -> Option<String> {
    let output = if cfg!(windows) {
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("(Get-Process -Id {pid}).PeakWorkingSet64"),
            ])
            .output()
            .ok()?
    } else {
        std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &pid.to_string()])
            .output()
            .ok()?
    };
    let value: u64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .ok()?;
    let bytes = if cfg!(windows) { value } else { value * 1024 };
    let label = if cfg!(windows) { "peak" } else { "now" };
    Some(format!("{:.0} MB ({label})", bytes as f64 / 1_048_576.0))
}

/// A mono or stereo WAV, 16-bit or float, mixed down to mono.
/// PowerShell passes `%LOCALAPPDATA%` through as written, so `%NAME%`
/// variables are expanded here.
fn expand_env(arg: &str) -> String {
    let mut out = String::new();
    let mut rest = arg;
    while let Some(start) = rest.find('%') {
        let Some(len) = rest[start + 1..].find('%') else {
            break;
        };
        let name = &rest[start + 1..start + 1 + len];
        out.push_str(&rest[..start]);
        match std::env::var(name) {
            Ok(value) if !name.is_empty() => out.push_str(&value),
            _ => out.push_str(&rest[start..start + len + 2]),
        }
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

/// Windows shells pass `rec\*.wav` through unexpanded, so a folder or a
/// `*.wav` pattern in the last path segment is expanded here.
fn expand(arg: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let path = Path::new(arg);
    let (dir, pattern) = if path.is_dir() {
        (path.to_path_buf(), "*.wav".to_string())
    } else if arg.contains('*') {
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        (dir.to_path_buf(), name)
    } else {
        return Ok(vec![arg.to_string()]);
    };
    let (prefix, suffix) = pattern.split_once('*').unwrap_or((pattern.as_str(), ""));
    let (prefix, suffix) = (prefix.to_lowercase(), suffix.to_lowercase());
    let mut found = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.starts_with(&prefix) && name.ends_with(&suffix) && entry.path().is_file() {
            found.push(entry.path().to_string_lossy().into_owned());
        }
    }
    found.sort();
    Ok(found)
}

fn read_wav(path: &str) -> Result<AudioClip, Box<dyn std::error::Error>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<Result<_, _>>()?
        }
    };
    let channels = usize::from(spec.channels.max(1));
    let samples = raw
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    Ok(AudioClip {
        samples,
        sample_rate: spec.sample_rate,
    })
}
