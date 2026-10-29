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
}

impl Teacher {
    pub const ALL: [Self; 4] = [Self::Helena, Self::Lia, Self::Rafael, Self::Otavio];

    /// The name the tutor gives itself in lessons.
    pub fn name(self) -> &'static str {
        match self {
            Self::Helena => "Helena",
            Self::Lia => "Lia",
            Self::Rafael => "Rafael",
            Self::Otavio => "Otávio",
        }
    }

    pub fn gender(self) -> VoiceGender {
        match self {
            Self::Helena | Self::Lia => VoiceGender::Female,
            Self::Rafael | Self::Otavio => VoiceGender::Male,
        }
    }

    /// The voice chosen with this teacher, when it is installed.
    pub fn voice(self) -> Voice {
        match self {
            Self::Helena => Voice::Kokoro(KokoroVoice::Dora),
            Self::Lia => Voice::Piper(PiperVoice::Dii),
            Self::Rafael => Voice::Piper(PiperVoice::Faber),
            Self::Otavio => Voice::Piper(PiperVoice::Cadu),
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

/// Reads a saved teacher, forgetting one that no longer exists (Thoth was
/// one until 0.25), so the settings still load and a teacher matching the
/// voice is used instead.
pub(crate) fn lenient<'de, D>(deserializer: D) -> Result<Option<Teacher>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|v| serde_json::from_value(v).ok()))
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

    #[test]
    fn a_removed_teacher_reads_as_none() {
        #[derive(Deserialize)]
        struct Saved {
            #[serde(default, deserialize_with = "lenient")]
            teacher: Option<Teacher>,
        }
        let read = |json: &str| serde_json::from_str::<Saved>(json).unwrap().teacher;
        assert_eq!(read(r#"{"teacher":"thoth"}"#), None);
        assert_eq!(read(r#"{"teacher":"lia"}"#), Some(Teacher::Lia));
        assert_eq!(read(r#"{"teacher":null}"#), None);
        assert_eq!(read("{}"), None);
    }
}
