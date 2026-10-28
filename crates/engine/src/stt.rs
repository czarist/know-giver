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

/// True when the transcript is at most one word.
pub(crate) fn needs_second_opinion(text: &str) -> bool {
    word_count(text) <= 1
}

pub(crate) fn word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
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
        assert!(!needs_second_opinion("Pode continuar."));
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
