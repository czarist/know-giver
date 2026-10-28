//! User settings, stored as JSON in the database.

use kg_ai::AiConfig;
use kg_audio::{CloudSpeechConfig, SttEngine, TtsBackend, Voice, WhisperModel};
use kg_study::{Store, StoreError};
use serde::{Deserialize, Serialize};

const SETTINGS_KEY: &str = "app_settings";

/// Teacher illustration style. Voice gender is applied independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AvatarStyle {
    #[default]
    Classic,
    Minimal,
}

/// Everything the user can change in Settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ai: AiConfig,
    /// Read tutor answers aloud.
    pub voice_enabled: bool,
    /// Start listening again after the tutor finishes speaking.
    pub hands_free: bool,
    /// Stop listening when the student goes quiet.
    pub auto_stop: bool,
    /// Speech recognizer; falls back to whichever is installed.
    pub stt_engine: SttEngine,
    /// Whisper model, used when Whisper is the recognizer and to double-check
    /// one-word answers with Parakeet.
    pub whisper_model: WhisperModel,
    pub voice: Voice,
    pub tts_backend: TtsBackend,
    pub cloud_speech: CloudSpeechConfig,
    pub speech_pitch: f32,
    /// Piper intonation variation (`noise_scale`).
    pub speech_expressiveness: f32,
    /// Piper rhythm variation (`noise_w`).
    pub speech_rhythm_variation: f32,
    pub avatar_style: AvatarStyle,
    /// Speech speed multiplier: 1.0 is normal, 1.2 is 20% faster.
    pub speech_rate: f32,
    pub quiz_size: u32,
    /// UI locale, e.g. "pt-BR".
    pub locale: String,
    /// The first-run welcome steps were closed.
    pub onboarding_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ai: AiConfig::default(),
            voice_enabled: true,
            hands_free: true,
            auto_stop: true,
            stt_engine: SttEngine::default(),
            whisper_model: WhisperModel::default(),
            voice: Voice::default(),
            tts_backend: TtsBackend::default(),
            cloud_speech: CloudSpeechConfig::default(),
            speech_pitch: 0.0,
            speech_expressiveness: kg_audio::DEFAULT_EXPRESSIVENESS,
            speech_rhythm_variation: kg_audio::DEFAULT_RHYTHM_VARIATION,
            avatar_style: AvatarStyle::default(),
            speech_rate: 0.95,
            quiz_size: 5,
            locale: "pt-BR".into(),
            onboarding_done: false,
        }
    }
}

impl Settings {
    /// Loads saved settings. Values from `.env` fill anything never saved,
    /// and the API key always comes from `.env` unless the user typed a
    /// different one.
    pub fn load(store: &Store, env_ai: &AiConfig) -> Result<Self, StoreError> {
        let saved = store.setting(SETTINGS_KEY)?.and_then(|json| {
            match serde_json::from_str::<Settings>(&json) {
                Ok(settings) => Some(settings),
                Err(err) => {
                    tracing::warn!(%err, "ignoring unreadable settings");
                    None
                }
            }
        });
        let mut settings = saved.unwrap_or_else(|| Self {
            ai: env_ai.clone(),
            ..Self::default()
        });
        if settings.ai.api_key.is_empty() {
            settings.ai.api_key = env_ai.api_key.clone();
        }
        settings.normalize();
        Ok(settings)
    }

    pub(crate) fn normalize(&mut self) {
        self.quiz_size = self.quiz_size.clamp(3, 10);
        self.speech_rate = if self.speech_rate.is_finite() {
            self.speech_rate.clamp(0.7, 1.5)
        } else {
            0.95
        };
        self.speech_pitch = if self.speech_pitch.is_finite() {
            self.speech_pitch.clamp(-6.0, 6.0)
        } else {
            0.0
        };
        let clamp = |value: f32, range: std::ops::RangeInclusive<f32>, default: f32| {
            if value.is_finite() {
                value.clamp(*range.start(), *range.end())
            } else {
                default
            }
        };
        self.speech_expressiveness = clamp(
            self.speech_expressiveness,
            kg_audio::EXPRESSIVENESS_RANGE,
            kg_audio::DEFAULT_EXPRESSIVENESS,
        );
        self.speech_rhythm_variation = clamp(
            self.speech_rhythm_variation,
            kg_audio::RHYTHM_VARIATION_RANGE,
            kg_audio::DEFAULT_RHYTHM_VARIATION,
        );
    }

    /// Saves settings. A key equal to the `.env` one is not copied into the
    /// database.
    pub fn save(&self, store: &mut Store, env_ai: &AiConfig) -> Result<(), StoreError> {
        let mut to_save = self.clone();
        if to_save.ai.api_key == env_ai.api_key {
            to_save.ai.api_key.clear();
        }
        let json = serde_json::to_string(&to_save).unwrap_or_default();
        store.set_setting(SETTINGS_KEY, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_ai() -> AiConfig {
        AiConfig {
            base_url: "https://api.example/v4".into(),
            api_key: "env-key".into(),
            ..AiConfig::default()
        }
    }

    #[test]
    fn first_run_uses_env() {
        let store = Store::open_in_memory().unwrap();
        let settings = Settings::load(&store, &env_ai()).unwrap();
        assert_eq!(settings.ai, env_ai());
        assert!(settings.voice_enabled);
    }

    #[test]
    fn env_key_is_not_stored_but_custom_key_is() {
        let mut store = Store::open_in_memory().unwrap();
        let mut settings = Settings::load(&store, &env_ai()).unwrap();
        settings.ai.base_url = "http://localhost:11434/v1".into();
        settings.save(&mut store, &env_ai()).unwrap();
        assert!(
            !store
                .setting(SETTINGS_KEY)
                .unwrap()
                .unwrap()
                .contains("env-key")
        );
        let loaded = Settings::load(&store, &env_ai()).unwrap();
        assert_eq!(loaded.ai.base_url, "http://localhost:11434/v1");
        assert_eq!(loaded.ai.api_key, "env-key");

        settings.ai.api_key = "typed-key".into();
        settings.save(&mut store, &env_ai()).unwrap();
        assert_eq!(
            Settings::load(&store, &env_ai()).unwrap().ai.api_key,
            "typed-key"
        );
    }
}
