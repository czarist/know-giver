//! Speech recognition for lessons: Parakeet (fast, live captions) or
//! Whisper, kept loaded between turns and shared with worker threads.

use std::sync::{Arc, Mutex};

use kg_audio::{
    AudioClip, AudioError, ModelId, ModelStore, ParakeetStt, SttEngine, WhisperModel, WhisperStt,
};

use crate::EngineError;

/// Language code passed to Whisper.
const STT_LANGUAGE: &str = "pt";

/// Loaded recognizers. Cheap to clone; clones share the loaded models.
#[derive(Debug, Clone, Default)]
pub(crate) struct Recognizers {
    whisper: Arc<Mutex<Option<(WhisperModel, WhisperStt)>>>,
    parakeet: Arc<Mutex<Option<ParakeetStt>>>,
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
        let whisper_installed = models.is_installed(ModelId::Whisper(whisper));
        let engine = match preferred {
            SttEngine::Parakeet if parakeet => SttEngine::Parakeet,
            SttEngine::Whisper if whisper_installed => SttEngine::Whisper,
            _ if parakeet => SttEngine::Parakeet,
            _ if whisper_installed => SttEngine::Whisper,
            _ => return None,
        };
        Some(Self {
            engine,
            recognizers: recognizers.clone(),
            models: models.clone(),
            whisper,
            whisper_installed,
        })
    }

    /// Whether live captions can be computed while the student speaks.
    pub(crate) fn live(&self) -> bool {
        self.engine == SttEngine::Parakeet
    }

    /// Final transcription of a turn. `context` is what the tutor said last.
    pub(crate) fn transcribe(
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
                if needs_second_opinion(&text)
                    && let Some(checked) = self.check_short_answer(clip, context)
                {
                    tracing::info!(parakeet = %text, whisper = %checked, "short answer checked");
                    return Ok(checked);
                }
                Ok(text)
            }
        }
    }

    /// Quick transcription of the audio so far, for live captions.
    pub(crate) fn partial(&self, clip: &AudioClip) -> Result<String, EngineError> {
        self.parakeet(clip)
    }

    /// Loads the models in the background so the first answer is not delayed.
    pub(crate) fn preload(&self) {
        let job = self.clone();
        let spawned = std::thread::Builder::new()
            .name("kg-stt-load".into())
            .spawn(move || {
                if job.engine == SttEngine::Parakeet {
                    let silence = AudioClip {
                        samples: Vec::new(),
                        sample_rate: kg_audio::STT_SAMPLE_RATE,
                    };
                    if let Err(err) = job.parakeet(&silence) {
                        tracing::warn!(%err, "parakeet preload failed");
                    }
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
const ENGLISH_WORDS: [&str; 48] = [
    "the", "that", "this", "not", "is", "it", "its", "you", "what", "yes", "no", "okay", "ok",
    "and", "of", "to", "so", "yeah", "oh", "my", "me", "we", "do", "dont", "im", "there", "they",
    "have", "are", "was", "be", "can", "just", "know", "like", "right", "well", "thank", "thanks",
    "sorry", "hello", "hi", "but", "for", "with", "nah", "sure", "why",
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

/// True when most words are common English ones and none settles it as
/// Portuguese. Technical terms ("machine learning") do not count either way.
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
    english > portuguese && english * 2 >= words.len()
}

/// True when `heard` has a meaningful word that is not in `known` (for
/// example the tutor's "Hum." coming back through the speakers).
pub(crate) fn has_new_words(heard: &str, known: &str) -> bool {
    let known: std::collections::HashSet<String> = content_words(known).into_iter().collect();
    content_words(heard).iter().any(|w| !known.contains(w))
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

/// True when `heard` (what the microphone picked up while the tutor was
/// speaking) is the student and not the tutor's own voice from the speakers:
/// at least two meaningful words, most of them not in what the tutor said.
pub(crate) fn is_student_speech(heard: &str, tutor: &str) -> bool {
    // Numbers are read out differently from how they are written.
    let heard: Vec<String> = content_words(heard)
        .into_iter()
        .filter(|w| !w.chars().any(|c| c.is_ascii_digit()))
        .collect();
    if heard.len() < 2 {
        return false;
    }
    // Both what is on screen ("art. 5º, CF/88") and what is spoken
    // ("artigo quinto, Constituição Federal").
    let spoken = kg_audio::normalize_for_speech(tutor);
    let tutor: std::collections::HashSet<String> = content_words(tutor)
        .into_iter()
        .chain(content_words(&spoken))
        .collect();
    let echoed = heard.iter().filter(|w| tutor.contains(*w)).count();
    echoed * 2 <= heard.len()
}

/// Lowercase words of three or more letters, without accents.
fn content_words(text: &str) -> Vec<String> {
    content_words_all(text)
        .into_iter()
        .filter(|w| w.chars().count() >= 3)
        .collect()
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
        assert!(!needs_second_opinion("Python é uma linguagem."));
        assert!(!needs_second_opinion("No caso, a LOA."));
        assert!(same_words("Sim, entendi.", "sim entendi"));
        assert!(!same_words("Sim", "Sim, entendi"));
        assert!(has_new_words("Hum, espera.", "Hum."));
        assert!(!has_new_words("Hum.", "Hum."));
    }

    #[test]
    fn tells_the_student_from_the_tutor_echo() {
        let tutor = "Antes de horas, usamos crase sempre que a expressão vier precedida de às. \
                     Por exemplo: a reunião começa às quatorze horas.";
        // The tutor's voice coming back through the speakers.
        assert!(!is_student_speech(
            "usamos crase sempre que a expressao",
            tutor
        ));
        assert!(!is_student_speech("reunião começa às quatorze", tutor));
        // The student talking over it.
        assert!(is_student_speech("Espera, não entendi essa parte", tutor));
        assert!(is_student_speech("Pera aí, e antes de dias?", tutor));
        // Too little to tell.
        assert!(!is_student_speech("Hum", tutor));
        // Abbreviations on screen are spoken in full.
        let legal =
            "Bom dia! Hoje vamos ver o art. 5º, § 2º, da CF/88 e a Lei nº 8.112/90 (servidores).";
        let echo = "Bom dia! Hoje vamos ver o artigo 5o, parágrafo 2o, da Constituição Federal de 1988 e a Lei número 8112 de 1990, servidores.";
        assert!(!is_student_speech(echo, legal));
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
        assert_eq!(resolve(SttEngine::Whisper), Some(SttEngine::Whisper));
    }
}
