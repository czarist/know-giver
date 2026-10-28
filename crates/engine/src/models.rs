//! Downloading and removing local speech models.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use kg_audio::{AudioError, ModelId, TtsBackend, Voice};

use crate::{Engine, EngineError, Event, Notice};

/// Install state of a model, for the Settings screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModelStatus {
    Missing,
    /// Fraction done, `0.0..=1.0`.
    Downloading(f32),
    Installed {
        bytes: u64,
    },
}

pub(crate) struct Download {
    /// Progress in millionths.
    progress: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
}

impl Engine {
    pub fn model_status(&self, id: ModelId) -> ModelStatus {
        if let Some(download) = self.downloads.get(&id) {
            return ModelStatus::Downloading(
                download.progress.load(Ordering::Relaxed) as f32 / 1_000_000.0,
            );
        }
        if self.models.is_installed(id) {
            ModelStatus::Installed {
                bytes: self.models.size_on_disk(id),
            }
        } else {
            ModelStatus::Missing
        }
    }

    pub(crate) fn download_model(&mut self, id: ModelId) {
        if self.downloads.contains_key(&id) || self.models.is_installed(id) {
            return;
        }
        let progress = Arc::new(AtomicU64::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        self.downloads.insert(
            id,
            Download {
                progress: Arc::clone(&progress),
                cancel: Arc::clone(&cancel),
            },
        );
        let models = self.models.clone();
        let (_, waker) = self.sender();
        self.spawn(move || {
            let report = |done: u64, total: u64| {
                let fraction = (done as f64 / total.max(1) as f64).min(1.0);
                let millionths = (fraction * 1_000_000.0) as u64;
                // Wake the UI about every 1% so the bar moves smoothly.
                if millionths / 10_000 != progress.load(Ordering::Relaxed) / 10_000 {
                    waker();
                }
                progress.store(millionths, Ordering::Relaxed);
            };
            Event::DownloadFinished(id, models.download(id, &report, &cancel))
        });
    }

    pub(crate) fn cancel_download(&mut self, id: ModelId) {
        if let Some(download) = self.downloads.get(&id) {
            download.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn on_download_finished(
        &mut self,
        id: ModelId,
        result: Result<(), AudioError>,
    ) -> Result<(), EngineError> {
        self.downloads.remove(&id);
        match result {
            Ok(()) => {
                self.voice.reset_tts();
                self.notices.push(Notice::ModelInstalled(id));
                Ok(())
            }
            Err(AudioError::Cancelled) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    pub(crate) fn remove_model(&mut self, id: ModelId) -> Result<(), EngineError> {
        if self.downloads.contains_key(&id) {
            self.cancel_download(id);
            return Ok(());
        }
        self.stop_voice_preview();
        self.stop_speaking();
        if let ModelId::Whisper(model) = id
            && let Ok(mut stt) = self.stt.lock()
            && stt.as_ref().is_some_and(|(loaded, _)| *loaded == model)
        {
            *stt = None;
        }
        self.voice.reset_tts();
        self.models.remove(id)?;
        Ok(())
    }

    /// True when speech recognition can run.
    pub fn stt_ready(&self) -> bool {
        self.models
            .is_installed(ModelId::Whisper(self.settings.whisper_model))
    }

    /// True when the tutor can speak.
    pub fn tts_ready(&self) -> bool {
        match self.settings.tts_backend {
            TtsBackend::Local => self.voice_available(self.settings.voice),
            TtsBackend::Cloud => {
                !self.settings.cloud_speech.base_url.trim().is_empty()
                    && !self.settings.cloud_speech.model.trim().is_empty()
                    && !self.settings.cloud_speech.voice.trim().is_empty()
            }
        }
    }

    /// True when both the selected local model and its runtime are installed.
    pub fn voice_available(&self, voice: Voice) -> bool {
        self.models.is_installed(voice.model_id()) && self.models.is_installed(voice.runtime_id())
    }
}
