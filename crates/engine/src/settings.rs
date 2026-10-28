//! User settings, stored as JSON in the database.

use kg_ai::AiConfig;
use kg_audio::{PiperVoice, WhisperModel};
use kg_study::{Store, StoreError};
use serde::{Deserialize, Serialize};

const SETTINGS_KEY: &str = "app_settings";

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
    pub whisper_model: WhisperModel,
    pub voice: PiperVoice,
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
            whisper_model: WhisperModel::default(),
            voice: PiperVoice::default(),
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
        settings.quiz_size = settings.quiz_size.clamp(3, 10);
        settings.speech_rate = settings.speech_rate.clamp(0.7, 1.5);
        Ok(settings)
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
