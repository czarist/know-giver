//! Generate a listening corpus with sherpa-onnx v1.12.20 and Kokoro v1.0.
//! See docs/kokoro-evaluation.md for the required local files and acceptance checks.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

// Speaker IDs belong specifically to the 53-speaker v1.0 bundle.
const VOICES: [(&str, u32); 3] = [("pf_dora", 42), ("pm_alex", 43), ("pm_santa", 44)];
const SAMPLES: [&str; 3] = [
    "Bom dia! Hoje vamos ver o art. 5º, § 2º, da CF/88 e a Lei nº 8.112/90 (servidores).",
    "A crase aparece na união de duas vogais. Pense na frase: vou à escola. Qual é a preposição usada pelo verbo?",
    "Uma taxa de 10% sobre R$ 200,00 corresponde a vinte reais. Agora me conte como você chegou a esse resultado.",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let [exe, model, output] = args.as_slice() else {
        return Err("usage: kokoro_check <sherpa-onnx-offline-tts> <kokoro-multi-lang-v1_0> <new-output-dir>".into());
    };
    let exe = std::fs::canonicalize(exe)?;
    let model = std::fs::canonicalize(model)?;
    validate_model(&model)?;
    // A fresh directory keeps previous listening results from being overwritten.
    std::fs::create_dir(output)?;
    let output = std::fs::canonicalize(output)?;
    for (voice, sid) in VOICES {
        for (index, text) in SAMPLES.iter().enumerate() {
            let text = kg_audio::normalize_for_speech(text);
            let stem = output.join(format!("{voice}-{}", index + 1));
            let wav = stem.with_extension("wav");
            std::fs::write(stem.with_extension("txt"), &text)?;
            let started = Instant::now();
            let result = command(&exe, &model, &wav, sid, &text).output()?;
            std::fs::write(stem.with_extension("stdout.log"), &result.stdout)?;
            std::fs::write(stem.with_extension("stderr.log"), &result.stderr)?;
            if !result.status.success() {
                return Err(format!(
                    "{voice} failed with {}; see {}",
                    result.status,
                    stem.with_extension("stderr.log").display()
                )
                .into());
            }
            if !wav.is_file() || std::fs::metadata(&wav)?.len() <= 44 {
                return Err(format!("no audio produced at {}", wav.display()).into());
            }
            println!(
                "{}: {:.2}s including model load",
                wav.display(),
                started.elapsed().as_secs_f64()
            );
        }
    }
    Ok(())
}

fn validate_model(model: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for file in [
        "model.onnx",
        "voices.bin",
        "tokens.txt",
        "espeak-ng-data/phontab",
        "espeak-ng-data/phonindex",
        "espeak-ng-data/phondata",
        "espeak-ng-data/intonations",
    ] {
        let path = model.join(file);
        if !path.is_file() || std::fs::metadata(&path)?.len() == 0 {
            return Err(format!("missing or empty Kokoro file: {}", path.display()).into());
        }
    }
    Ok(())
}

fn path_arg(name: &str, path: PathBuf) -> OsString {
    let mut arg = OsString::from(format!("--{name}="));
    arg.push(path);
    arg
}

fn command(exe: &Path, model: &Path, output: &Path, sid: u32, text: &str) -> Command {
    let mut command = Command::new(exe);
    command
        .arg(path_arg("kokoro-model", model.join("model.onnx")))
        .arg(path_arg("kokoro-voices", model.join("voices.bin")))
        .arg(path_arg("kokoro-tokens", model.join("tokens.txt")))
        .arg(path_arg("kokoro-data-dir", model.join("espeak-ng-data")))
        .arg("--kokoro-lang=pt-br")
        .arg("--kokoro-length-scale=1.052632") // Same 0.95 speed as the app.
        .arg("--num-threads=2")
        .arg("--provider=cpu")
        .arg("--debug=0")
        .arg(format!("--sid={sid}"))
        .arg(path_arg("output-filename", output.to_path_buf()))
        .arg(text)
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_text_are_individual_arguments() {
        let model = Path::new("models with spaces/ação");
        let text = "Uma frase com \"aspas\" e acentos: à escola.";
        let command = command(
            Path::new("sherpa.exe"),
            model,
            Path::new("out dir/a.wav"),
            42,
            text,
        );
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args[0], path_arg("kokoro-model", model.join("model.onnx")));
        assert_eq!(args.last().copied(), Some(std::ffi::OsStr::new(text)));
        assert!(args.contains(&std::ffi::OsStr::new("--sid=42")));
        assert!(args.contains(&std::ffi::OsStr::new("--kokoro-lang=pt-br")));
    }

    #[test]
    fn incomplete_model_is_rejected_before_starting_process() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("model.onnx"), b"model").unwrap();
        let error = validate_model(dir.path()).unwrap_err().to_string();
        assert!(error.contains("voices.bin"), "{error}");
    }
}
