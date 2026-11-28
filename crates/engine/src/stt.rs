//! Speech recognition for lessons: Parakeet (fast, live captions), Nemotron
//! (told the language; being tested as Parakeet's replacement) or Whisper,
//! kept loaded between turns and shared with worker threads.

use std::sync::{Arc, Mutex};

use kg_audio::{
    AudioClip, AudioError, ModelId, ModelStore, NemotronStt, ParakeetStt, SttEngine, WhisperModel,
    WhisperStt,
};

use crate::EngineError;

/// Language code passed to Whisper.
const STT_LANGUAGE: &str = "pt";

/// Loaded recognizers. Cheap to clone; clones share the loaded models.
#[derive(Debug, Clone, Default)]
pub(crate) struct Recognizers {
    whisper: Arc<Mutex<Option<(WhisperModel, WhisperStt)>>>,
    parakeet: Arc<Mutex<Option<ParakeetStt>>>,
    nemotron: Arc<Mutex<Option<NemotronStt>>>,
}

impl Recognizers {
    /// Forgets a model that is being removed from disk.
    pub(crate) fn unload(&self, id: ModelId) {
        match id {
            ModelId::Whisper(model) => {
                if let Ok(mut whisper) = self.whisper.lock()
                    && whisper.as_ref().is_some_and(|(loaded, _)| *loaded == model)
                {
                    *whisper = None;
                }
            }
            ModelId::Parakeet | ModelId::KokoroRuntime => {
                if let Ok(mut parakeet) = self.parakeet.lock() {
                    *parakeet = None;
                }
            }
            ModelId::Nemotron | ModelId::NemoSpeechRuntime => {
                if let Ok(mut nemotron) = self.nemotron.lock() {
                    *nemotron = None;
                }
            }
            _ => {}
        }
    }
}

/// What a transcription runs with, decided on the UI thread from settings
/// and installed models.
#[derive(Debug, Clone)]
pub(crate) struct SttJob {
    pub(crate) engine: SttEngine,
    recognizers: Recognizers,
    models: ModelStore,
    whisper: WhisperModel,
    whisper_installed: bool,
    parakeet_installed: bool,
}

impl SttJob {
    /// The recognizer to use: the chosen one when installed, otherwise
    /// whichever is. `None` when neither can run.
    pub(crate) fn resolve(
        recognizers: &Recognizers,
        models: &ModelStore,
        preferred: SttEngine,
        whisper: WhisperModel,
    ) -> Option<Self> {
        let parakeet = models.is_installed(ModelId::Parakeet)
            && models.is_installed(ModelId::KokoroRuntime)
            && models.asr_server_exe().is_some();
        let nemotron = models.is_installed(ModelId::Nemotron)
            && models.is_installed(ModelId::NemoSpeechRuntime);
        let whisper_installed = models.is_installed(ModelId::Whisper(whisper));
        let engine = match preferred {
            SttEngine::Parakeet if parakeet => SttEngine::Parakeet,
            SttEngine::Nemotron if nemotron => SttEngine::Nemotron,
            SttEngine::Whisper if whisper_installed => SttEngine::Whisper,
            _ if parakeet => SttEngine::Parakeet,
            _ if whisper_installed => SttEngine::Whisper,
            _ if nemotron => SttEngine::Nemotron,
            _ => return None,
        };
        Some(Self {
            engine,
            recognizers: recognizers.clone(),
            models: models.clone(),
            whisper,
            whisper_installed,
            parakeet_installed: parakeet,
        })
    }

    /// Whether every final transcript ends up in Portuguese: Whisper and
    /// Nemotron are told the language, Parakeet is not and needs Whisper to
    /// read again what came out in another language.
    pub(crate) fn keeps_portuguese(&self) -> bool {
        self.engine != SttEngine::Parakeet || self.whisper_installed
    }

    /// Whether live captions can be computed while the student speaks.
    pub(crate) fn live(&self) -> bool {
        matches!(self.engine, SttEngine::Parakeet | SttEngine::Nemotron)
    }

    /// Final transcription of a turn. `context` is what the tutor said last.
    /// When the recognizer finds no words in audio the recorder took for
    /// speech, another installed recognizer reads it once more: Nemotron
    /// sometimes returns nothing for clear speech.
    pub(crate) fn transcribe(
        &self,
        clip: &AudioClip,
        context: &str,
    ) -> Result<String, EngineError> {
        let text = self.transcribe_with_chosen(clip, context)?;
        if !text.trim().is_empty() {
            return Ok(text);
        }
        match self.second_try(clip, context) {
            Some(checked) => {
                tracing::info!(engine = ?self.engine, text = %checked, "empty transcript read again");
                Ok(checked)
            }
            None => Ok(text),
        }
    }

    /// Another installed recognizer over a clip the chosen one found empty.
    fn second_try(&self, clip: &AudioClip, context: &str) -> Option<String> {
        if self.engine != SttEngine::Whisper
            && let Some(text) = self.retranscribe(clip, context)
        {
            return Some(text);
        }
        if self.engine != SttEngine::Parakeet && self.parakeet_installed {
            match self.parakeet(clip) {
                Ok(text) if !text.trim().is_empty() => return Some(text),
                Ok(_) => {}
                Err(err) => tracing::warn!(%err, "parakeet second try failed"),
            }
        }
        None
    }

    fn transcribe_with_chosen(
        &self,
        clip: &AudioClip,
        context: &str,
    ) -> Result<String, EngineError> {
        match self.engine {
            SttEngine::Whisper => self.with_whisper(|whisper| {
                Ok(whisper.transcribe_with_context(clip, STT_LANGUAGE, context)?)
            }),
            SttEngine::Parakeet => {
                let text = self.parakeet(clip)?;
                if has_foreign_script(&text)
                    && let Some(checked) = self.retranscribe(clip, context)
                {
                    tracing::info!(parakeet = %text, whisper = %checked, "wrong alphabet retranscribed");
                    return Ok(checked);
                }
                // Parakeet guesses the language itself; Whisper is told it is
                // Portuguese, so English it heard is read again in full.
                if has_english(&text)
                    && word_count(&text) > 1
                    && let Some(checked) = self.retranscribe(clip, context)
                {
                    tracing::info!(parakeet = %text, whisper = %checked, "english retranscribed");
                    return Ok(checked);
                }
                if needs_second_opinion(&text)
                    && let Some(checked) = self.check_short_answer(clip, context)
                {
                    tracing::info!(parakeet = %text, whisper = %checked, "short answer checked");
                    return Ok(checked);
                }
                Ok(text)
            }
            // Told the language, so no Whisper second opinion: the point of
            // the test is to see how it does on its own.
            SttEngine::Nemotron => self.nemotron(clip),
        }
    }

    /// Quick transcription of the audio so far, for live captions.
    pub(crate) fn partial(&self, clip: &AudioClip) -> Result<String, EngineError> {
        match self.engine {
            SttEngine::Nemotron => self.nemotron(clip),
            _ => self.parakeet(clip),
        }
    }

    /// Loads the models in the background so the first answer is not delayed.
    pub(crate) fn preload(&self) {
        let job = self.clone();
        let spawned = std::thread::Builder::new()
            .name("kg-stt-load".into())
            .spawn(move || {
                let silence = AudioClip {
                    samples: Vec::new(),
                    sample_rate: kg_audio::STT_SAMPLE_RATE,
                };
                if job.engine == SttEngine::Parakeet
                    && let Err(err) = job.parakeet(&silence)
                {
                    tracing::warn!(%err, "parakeet preload failed");
                }
                if job.engine == SttEngine::Nemotron
                    && let Err(err) = job.nemotron(&silence)
                {
                    tracing::warn!(%err, "nemotron preload failed");
                }
                // Whisper also double-checks one-word answers.
                if job.whisper_installed
                    && let Err(err) = job.with_whisper(|_| Ok(()))
                {
                    tracing::warn!(%err, "whisper preload failed");
                }
            });
        if let Err(err) = spawned {
            tracing::warn!(%err, "could not spawn speech model preload");
        }
    }

    /// One-word answers ("sim", "não") are where Parakeet most often picks
    /// the wrong language. Whisper, told the language and what the tutor
    /// asked, reads them better; with a short window it costs under a second.
    fn check_short_answer(&self, clip: &AudioClip, context: &str) -> Option<String> {
        if !self.whisper_installed {
            return None;
        }
        match self.with_whisper(|whisper| {
            Ok(whisper.transcribe_short_with_context(clip, STT_LANGUAGE, context)?)
        }) {
            Ok(text) if !text.trim().is_empty() => Some(text),
            Ok(_) => None,
            Err(err) => {
                tracing::warn!(%err, "whisper check of a short answer failed");
                None
            }
        }
    }

    /// Whisper over the whole answer, for when Parakeet heard another
    /// language entirely ("только стоми"). Slower than the short check, but
    /// the answer may be a full sentence.
    fn retranscribe(&self, clip: &AudioClip, context: &str) -> Option<String> {
        if !self.whisper_installed {
            return None;
        }
        match self.with_whisper(|whisper| {
            Ok(whisper.transcribe_with_context(clip, STT_LANGUAGE, context)?)
        }) {
            Ok(text) if !text.trim().is_empty() => Some(text),
            Ok(_) => None,
            Err(err) => {
                tracing::warn!(%err, "whisper retranscription failed");
                None
            }
        }
    }

    /// Runs Parakeet, starting its server when needed and once more if it
    /// stopped answering.
    fn parakeet(&self, clip: &AudioClip) -> Result<String, EngineError> {
        let exe = self
            .models
            .asr_server_exe()
            .ok_or(EngineError::SpeechModelMissing(ModelId::KokoroRuntime))?;
        let mut guard = self
            .recognizers
            .parakeet
            .lock()
            .map_err(|_| AudioError::Model("speech server lock poisoned".into()))?;
        let mut retried = false;
        loop {
            if guard.as_mut().is_none_or(|p| !p.is_running()) {
                *guard = None;
                *guard = Some(ParakeetStt::start(&exe, &self.models.parakeet_dir())?);
            }
            let Some(parakeet) = guard.as_mut() else {
                continue;
            };
            match parakeet.transcribe(clip) {
                Ok(text) => return Ok(text),
                Err(err) if !retried => {
                    tracing::warn!(%err, "speech server failed; restarting it");
                    *guard = None;
                    retried = true;
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    /// Runs Nemotron, starting its server when needed and once more if it
    /// stopped answering.
    fn nemotron(&self, clip: &AudioClip) -> Result<String, EngineError> {
        let runtimes = self.models.nemo_speech_runtimes();
        if runtimes.is_empty() {
            return Err(EngineError::SpeechModelMissing(ModelId::NemoSpeechRuntime));
        }
        let model = self
            .models
            .nemotron_model()
            .ok_or(EngineError::SpeechModelMissing(ModelId::Nemotron))?;
        let mut guard = self
            .recognizers
            .nemotron
            .lock()
            .map_err(|_| AudioError::Model("speech server lock poisoned".into()))?;
        let mut retried = false;
        loop {
            if guard.as_mut().is_none_or(|n| !n.is_running()) {
                *guard = None;
                // Installed before builds were timed: time them once now.
                self.models.ensure_nemotron_timed();
                let runtimes = self.models.nemo_speech_runtimes();
                *guard = Some(NemotronStt::start(&runtimes, &model)?);
            }
            let Some(nemotron) = guard.as_mut() else {
                continue;
            };
            match nemotron.transcribe(clip) {
                Ok(text) => return Ok(text),
                Err(err) if !retried => {
                    tracing::warn!(%err, "nemotron server failed; restarting it");
                    *guard = None;
                    retried = true;
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    fn with_whisper<T>(
        &self,
        f: impl FnOnce(&mut WhisperStt) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        let mut guard = self
            .recognizers
            .whisper
            .lock()
            .map_err(|_| AudioError::Model("speech model lock poisoned".into()))?;
        if guard
            .as_ref()
            .is_none_or(|(loaded, _)| *loaded != self.whisper)
        {
            *guard = None;
            let whisper = WhisperStt::load(&self.models.whisper_dir(self.whisper))?;
            *guard = Some((self.whisper, whisper));
        }
        match guard.as_mut() {
            Some((_, whisper)) => f(whisper),
            None => Err(EngineError::SpeechModelMissing(ModelId::Whisper(
                self.whisper,
            ))),
        }
    }
}

/// Usual one-word answers in Portuguese. When Parakeet hears one of these,
/// it got the language right and Whisper is not needed.
const SHORT_ANSWERS: [&str; 16] = [
    "sim",
    "nao",
    "isso",
    "entendi",
    "certo",
    "claro",
    "exato",
    "exatamente",
    "beleza",
    "pode",
    "continua",
    "continue",
    "talvez",
    "obrigado",
    "obrigada",
    "nenhuma",
];

/// Common English words. Parakeet guesses the language by itself and hears
/// short or soft Portuguese as English: "Nada, nada." came out "Not that".
const ENGLISH_WORDS: [&str; 61] = [
    "now", "how", "on", "in", "at", "he", "she", "if", "or", "up", "out", "all", "one", "the",
    "that", "this", "not", "is", "it", "its", "you", "what", "yes", "no", "okay", "ok", "and",
    "of", "to", "so", "yeah", "oh", "my", "me", "we", "do", "dont", "im", "there", "they", "have",
    "are", "was", "be", "can", "just", "know", "like", "right", "well", "thank", "thanks", "sorry",
    "hello", "hi", "but", "for", "with", "nah", "sure", "why",
];

/// Common Portuguese words, which settle the language when both appear.
const PORTUGUESE_WORDS: [&str; 40] = [
    "de", "que", "nao", "e", "um", "uma", "do", "da", "em", "para", "pra", "com", "eu", "se", "na",
    "no", "isso", "mas", "por", "mais", "como", "sim", "ele", "ela", "voce", "ta", "entao", "nada",
    "tambem", "porque", "quando", "qual", "muito", "ja", "so", "meu", "os", "as", "dos", "aqui",
];

/// True when the transcript may be wrong and is worth a second opinion: a
/// lone unusual word ("Okay", "No"), or text that came out in English.
pub(crate) fn needs_second_opinion(text: &str) -> bool {
    if looks_english(text) {
        return true;
    }
    if word_count(text) > 1 {
        return false;
    }
    let word: String = content_words_all(text).concat();
    !SHORT_ANSWERS.contains(&word.as_str())
}

/// English words that are never Portuguese, even without accents ("to" is
/// "tô", "so" is "só", "for" is a verb form, "no" and "do" are contractions,
/// "ok" is used in Portuguese).
const ONLY_ENGLISH_WORDS: [&str; 48] = [
    "now", "how", "on", "in", "at", "he", "she", "if", "or", "up", "out", "all", "one", "the",
    "that", "this", "not", "is", "it", "its", "you", "what", "yes", "and", "of", "yeah", "my",
    "we", "dont", "im", "there", "they", "have", "are", "was", "be", "can", "just", "know", "like",
    "right", "well", "thank", "thanks", "sorry", "hello", "but", "with",
];

/// True when Parakeet wrote English: a word that only exists in English,
/// even among Portuguese ones ("eu acho que the lei"), or text that reads as
/// English. Technical terms ("machine learning") do not count.
pub(crate) fn has_english(text: &str) -> bool {
    looks_english(text)
        || content_words_all(text)
            .iter()
            .any(|w| ONLY_ENGLISH_WORDS.contains(&w.as_str()))
}

/// True when a letter is outside the Latin alphabet (Cyrillic, Greek, CJK,
/// …): Parakeet guessed a language the student is not speaking.
pub(crate) fn has_foreign_script(text: &str) -> bool {
    text.chars().any(|c| {
        c.is_alphabetic()
            && !matches!(c,
                'a'..='z' | 'A'..='Z'
                // Latin-1 letters and Latin Extended-A/B (ç, ã, é, ñ, …).
                | '\u{00C0}'..='\u{024F}'
                | 'ª' | 'º')
    })
}

/// True when a common English word appears and no common Portuguese one
/// settles it ("Ah now legal."). A false alarm only costs a Whisper check. Technical terms ("machine learning") do not count either way.
fn looks_english(text: &str) -> bool {
    let words = content_words_all(text);
    let english = words
        .iter()
        .filter(|w| ENGLISH_WORDS.contains(&w.as_str()))
        .count();
    let portuguese = words
        .iter()
        .filter(|w| PORTUGUESE_WORDS.contains(&w.as_str()))
        .count();
    english > 0 && portuguese == 0
}

/// Same text, ignoring case, accents and punctuation.
pub(crate) fn same_words(a: &str, b: &str) -> bool {
    content_words_all(a) == content_words_all(b)
}

pub(crate) fn word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
}

/// Lowercase words without accents.
fn content_words_all(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
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
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// True when the text ends a sentence, so the student has likely finished.
pub(crate) fn ends_sentence(text: &str) -> bool {
    let text = text.trim_end_matches(['"', '\'', ')', '»', '”']);
    text.ends_with(['.', '?', '!'])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 16-bit PCM WAV, any channel count, mixed to mono.
    fn read_pcm16(path: &std::path::Path) -> AudioClip {
        let bytes = std::fs::read(path).unwrap();
        let channels = u16::from_le_bytes([bytes[22], bytes[23]]) as usize;
        let sample_rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
        let data = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
        let samples: Vec<f32> = bytes[data..]
            .chunks_exact(2 * channels)
            .map(|frame| {
                frame
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|s| f32::from(i16::from_le_bytes(*s)) / 32768.0)
                    .sum::<f32>()
                    / channels as f32
            })
            .collect();
        AudioClip {
            samples,
            sample_rate,
        }
    }

    /// Manual: a models folder with Nemotron and its runtime installed (it
    /// is changed: the runtime is removed at the end) and the `rec` phrases.
    /// KG_NEMOTRON_MODELS=<dir> cargo test -p kg-engine -- --ignored nemotron_end_to_end
    #[test]
    #[ignore = "needs a Nemotron install; removes its runtime"]
    fn nemotron_end_to_end() {
        let dir = std::env::var("KG_NEMOTRON_MODELS").expect("KG_NEMOTRON_MODELS");
        let rec = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rec");
        let models = ModelStore::new(dir);
        let recognizers = Recognizers::default();
        let job = SttJob::resolve(
            &recognizers,
            &models,
            SttEngine::Nemotron,
            WhisperModel::default(),
        )
        .unwrap();
        assert_eq!(job.engine, SttEngine::Nemotron);
        assert!(job.live() && job.keeps_portuguese());

        // Final transcript, with the server started on first use.
        let clip = read_pcm16(&rec.join("07.wav"));
        let text = job.transcribe(&clip, "").unwrap();
        assert!(text.contains("legalidade"), "{text}");
        // Short answer.
        let text = job
            .transcribe(&read_pcm16(&rec.join("01.wav")), "")
            .unwrap();
        assert!(text.starts_with("Sim"), "{text}");
        // Live caption of the first half.
        let half = AudioClip {
            samples: clip.samples[..clip.samples.len() / 2].to_vec(),
            sample_rate: clip.sample_rate,
        };
        let partial = job.partial(&half).unwrap();
        assert!(partial.contains("princípio"), "{partial}");
        // Silence gives nothing, without an error.
        let silence = AudioClip {
            samples: vec![0.0; 16_000],
            sample_rate: 16_000,
        };
        assert_eq!(job.transcribe(&silence, "").unwrap(), "");
        // Several turns at once share one server.
        let threads: Vec<_> = (0..3)
            .map(|_| {
                let job = job.clone();
                let clip = clip.clone();
                std::thread::spawn(move || job.transcribe(&clip, ""))
            })
            .collect();
        for thread in threads {
            assert!(thread.join().unwrap().unwrap().contains("legalidade"));
        }
        // A server that died between turns is started again.
        let pid = recognizers
            .nemotron
            .lock()
            .unwrap()
            .as_mut()
            .map(|n| n.pid())
            .unwrap();
        let killed = if cfg!(windows) {
            std::process::Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .output()
        } else {
            std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .output()
        };
        assert!(killed.unwrap().status.success());
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(job.transcribe(&clip, "").unwrap().contains("legalidade"));
        // The runtime can be removed while its server runs (Windows keeps
        // running executables locked), as the settings screen does.
        recognizers.unload(ModelId::NemoSpeechRuntime);
        models.remove(ModelId::NemoSpeechRuntime).unwrap();
        assert!(!models.is_installed(ModelId::NemoSpeechRuntime));
        assert!(models.nemotron_fastest().is_none());
        assert_eq!(
            SttJob::resolve(
                &recognizers,
                &models,
                SttEngine::Nemotron,
                WhisperModel::default()
            )
            .map(|job| job.engine),
            None
        );
    }

    #[test]
    fn counts_words_not_punctuation() {
        assert_eq!(word_count(" Sim. "), 1);
        assert_eq!(word_count("Sim, entendi."), 2);
        assert_eq!(word_count(" - "), 0);
        assert!(needs_second_opinion("Okay."));
        assert!(needs_second_opinion("No,"));
        assert!(!needs_second_opinion("Sim."));
        assert!(!needs_second_opinion("Não."));
        assert!(!needs_second_opinion("Pode continuar."));
        assert!(needs_second_opinion("Not that, not."));
        assert!(needs_second_opinion("Yeah, I know that."));
        assert!(!needs_second_opinion("Não sei o que é machine learning."));
        // Heard in a lesson: Portuguese read as English words.
        assert!(needs_second_opinion("Ah now legal."));
        assert!(!needs_second_opinion(
            "O Estado fiscaliza as políticas públicas."
        ));
        assert!(!needs_second_opinion("Estado é permanente."));
        assert!(!needs_second_opinion("Python é uma linguagem."));
        assert!(!needs_second_opinion("No caso, a LOA."));
        assert!(same_words("Sim, entendi.", "sim entendi"));
        assert!(!same_words("Sim", "Sim, entendi"));
    }

    #[test]
    fn spots_english_among_portuguese() {
        assert!(has_english("Eu acho que the lei é federal."));
        assert!(has_english("Yeah, I know that."));
        assert!(has_english("Ah now legal."));
        assert!(!has_english("Não sei o que é machine learning."));
        assert!(!has_english("No caso, a LOA."));
        assert!(!has_english("Tô ok, só um pouco cansado."));
        assert!(!has_english("Se ele for aprovado, do jeito que está."));
        assert!(!has_english("Python é uma linguagem."));
    }

    #[test]
    fn spots_another_alphabet() {
        // Heard in a lesson: a Portuguese answer read as Russian.
        assert!(has_foreign_script("только стоми"));
        assert!(has_foreign_script("Sim, это"));
        assert!(!has_foreign_script("Ação, coração, pinguim, 5º lugar."));
        assert!(!has_foreign_script("Max Weber — dominação tradicional!"));
    }

    #[test]
    fn sentence_endings() {
        assert!(ends_sentence("Entendi."));
        assert!(ends_sentence("Pode repetir?"));
        assert!(ends_sentence("Ele disse \"sim.\""));
        assert!(!ends_sentence("Eu acho que"));
        assert!(!ends_sentence("Então,"));
    }

    #[test]
    fn falls_back_to_what_is_installed() {
        let dir = tempfile::tempdir().unwrap();
        let models = ModelStore::new(dir.path());
        let recognizers = Recognizers::default();
        let resolve = |preferred| {
            SttJob::resolve(&recognizers, &models, preferred, WhisperModel::Tiny)
                .map(|job| job.engine)
        };
        assert_eq!(resolve(SttEngine::Parakeet), None);
        let whisper = models.whisper_dir(WhisperModel::Tiny);
        std::fs::create_dir_all(&whisper).unwrap();
        for file in ["config.json", "tokenizer.json", "model.safetensors"] {
            std::fs::write(whisper.join(file), b"x").unwrap();
        }
        // Parakeet is not installed, so Whisper is used.
        assert_eq!(resolve(SttEngine::Parakeet), Some(SttEngine::Whisper));
        assert_eq!(resolve(SttEngine::Nemotron), Some(SttEngine::Whisper));
        assert_eq!(resolve(SttEngine::Whisper), Some(SttEngine::Whisper));
    }
}
