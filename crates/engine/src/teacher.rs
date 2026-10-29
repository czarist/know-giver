//! Who teaches the lessons: a portrait, a name the tutor uses for itself,
//! and the voice that fits it.

use kg_audio::{KokoroVoice, PiperVoice, Voice, VoiceGender};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Teacher {
    Helena,
    Lia,
    Rafael,
    Otavio,
    /// The app's patron, from the engraving on the covers.
    Thoth,
}

impl Teacher {
    pub const ALL: [Self; 5] = [
        Self::Helena,
        Self::Lia,
        Self::Rafael,
        Self::Otavio,
        Self::Thoth,
    ];

    /// The name the tutor gives itself in lessons.
    pub fn name(self) -> &'static str {
        match self {
            Self::Helena => "Helena",
            Self::Lia => "Lia",
            Self::Rafael => "Rafael",
            Self::Otavio => "Otávio",
            Self::Thoth => "Thoth",
        }
    }

    pub fn gender(self) -> VoiceGender {
        match self {
            Self::Helena | Self::Lia => VoiceGender::Female,
            Self::Rafael | Self::Otavio | Self::Thoth => VoiceGender::Male,
        }
    }

    /// The voice chosen with this teacher, when it is installed.
    pub fn voice(self) -> Voice {
        match self {
            Self::Helena => Voice::Kokoro(KokoroVoice::Dora),
            Self::Lia => Voice::Piper(PiperVoice::Dii),
            Self::Rafael => Voice::Piper(PiperVoice::Faber),
            Self::Otavio => Voice::Piper(PiperVoice::Cadu),
            Self::Thoth => Voice::Kokoro(KokoroVoice::Santa),
        }
    }

    /// Default for settings saved before teachers existed: one that matches
    /// the voice the student already picked.
    pub fn for_gender(gender: VoiceGender) -> Self {
        match gender {
            VoiceGender::Female => Self::Helena,
            VoiceGender::Male => Self::Rafael,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_voice_matches_the_teacher() {
        for teacher in Teacher::ALL {
            assert_eq!(teacher.voice().gender(), teacher.gender(), "{teacher:?}");
        }
        assert_eq!(
            serde_json::to_string(&Teacher::Otavio).unwrap(),
            "\"otavio\""
        );
    }
}
