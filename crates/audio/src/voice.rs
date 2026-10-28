//! Voice selection shared by the audio engine and settings.

use serde::{Deserialize, Serialize};

use crate::{ModelId, PiperVoice};

/// Voice presentation used by the picker and teacher avatar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VoiceGender {
    Female,
    #[default]
    Male,
}

/// Portuguese voices in the 53-speaker Kokoro v1.0 bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum KokoroVoice {
    #[default]
    #[serde(rename = "pf_dora")]
    Dora,
    #[serde(rename = "pm_alex")]
    Alex,
    #[serde(rename = "pm_santa")]
    Santa,
}

impl KokoroVoice {
    pub const ALL: [Self; 3] = [Self::Dora, Self::Alex, Self::Santa];

    pub fn id(self) -> &'static str {
        match self {
            Self::Dora => "pf_dora",
            Self::Alex => "pm_alex",
            Self::Santa => "pm_santa",
        }
    }

    /// Speaker index in the pinned Kokoro v1.0 model; not valid for v1.1.
    pub fn speaker_id(self) -> u32 {
        match self {
            Self::Dora => 42,
            Self::Alex => 43,
            Self::Santa => 44,
        }
    }
}

/// Installed local voice. The wire format retains old Piper settings unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Voice {
    Piper(PiperVoice),
    Kokoro(KokoroVoice),
}

impl Default for Voice {
    fn default() -> Self {
        Self::Piper(PiperVoice::default())
    }
}

impl Voice {
    pub const ALL: [Self; 6] = [
        Self::Piper(PiperVoice::Faber),
        Self::Piper(PiperVoice::Cadu),
        Self::Piper(PiperVoice::Jeff),
        Self::Kokoro(KokoroVoice::Dora),
        Self::Kokoro(KokoroVoice::Alex),
        Self::Kokoro(KokoroVoice::Santa),
    ];

    pub fn gender(self) -> VoiceGender {
        match self {
            Self::Kokoro(KokoroVoice::Dora) => VoiceGender::Female,
            _ => VoiceGender::Male,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Piper(v) => v.id(),
            Self::Kokoro(v) => v.id(),
        }
    }

    pub fn model_id(self) -> ModelId {
        match self {
            Self::Piper(v) => ModelId::Voice(v),
            Self::Kokoro(_) => ModelId::KokoroModel,
        }
    }

    pub fn runtime_id(self) -> ModelId {
        match self {
            Self::Piper(_) => ModelId::PiperRuntime,
            Self::Kokoro(_) => ModelId::KokoroRuntime,
        }
    }
}

/// Speech stays on the computer unless the cloud backend is selected explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TtsBackend {
    #[default]
    Local,
    Cloud,
}

/// OpenAI-compatible speech endpoint; selecting it sends tutor text to this URL.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudSpeechConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub voice: String,
    pub gender: VoiceGender,
}

impl std::fmt::Debug for CloudSpeechConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudSpeechConfig")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("voice", &self.voice)
            .field("gender", &self.gender)
            .finish_non_exhaustive()
    }
}

impl Default for CloudSpeechConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8880/v1".into(),
            api_key: String::new(),
            model: "kokoro".into(),
            voice: "pf_dora".into(),
            gender: VoiceGender::Female,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_piper_voices_survive_upgrade() {
        for (json, expected) in [
            ("\"faber\"", Voice::Piper(PiperVoice::Faber)),
            ("\"pf_dora\"", Voice::Kokoro(KokoroVoice::Dora)),
        ] {
            let voice: Voice = serde_json::from_str(json).unwrap();
            assert_eq!(voice, expected);
            assert_eq!(serde_json::to_string(&voice).unwrap(), json);
        }
    }

    #[test]
    fn cloud_debug_does_not_expose_key() {
        let config = CloudSpeechConfig {
            api_key: "secret-token".into(),
            ..CloudSpeechConfig::default()
        };
        assert!(!format!("{config:?}").contains("secret-token"));
    }
}
