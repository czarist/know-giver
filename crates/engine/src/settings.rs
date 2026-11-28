//! User settings, stored as JSON in the database.

use kg_ai::AiConfig;
use kg_audio::{
    CloudSpeechConfig, EnglishVoice, SttEngine, TtsBackend, Voice, VoiceGender, WhisperModel,
};
use kg_study::{Store, StoreError};
use serde::{Deserialize, Serialize};

const SETTINGS_KEY: &str = "app_settings";

/// Version of the required first-run setup (language, name, teacher, AI,
/// terms). Raising it asks every user to go through the setup again; study
/// data is never touched.
pub const SETUP_VERSION: u32 = 1;

/// Date of the terms of use and privacy policy in `docs/legal`. A new date
/// asks for agreement again.
pub const TERMS_VERSION: &str = "2026-11-28.1";

/// Where the student uses the app; the terms follow its law. The app is
/// offered to the public only in the listed regions; elsewhere ("other"),
/// only to partners Trentin authorized in writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    Br,
    Us,
    Eu,
    Uk,
    Jp,
    /// South Korea.
    Kr,
    Tw,
    Hk,
    Sg,
    /// Mainland China: authorized partners only, never offered to the public.
    Cn,
    Other,
}

impl Region {
    /// How people study for exams here, for the AI's teaching tasks; `None`
    /// for "other", where only the exam notice guides it.
    pub fn exam_culture(self) -> Option<kg_ai::ExamCulture> {
        use kg_ai::ExamCulture as C;
        Some(match self {
            Self::Br => C::Brazil,
            Self::Us => C::UnitedStates,
            Self::Eu => C::EuropeanUnion,
            Self::Uk => C::UnitedKingdom,
            Self::Jp => C::Japan,
            Self::Kr => C::SouthKorea,
            Self::Tw => C::Taiwan,
            Self::Hk => C::HongKong,
            Self::Sg => C::Singapore,
            Self::Cn => C::MainlandChina,
            Self::Other => return None,
        })
    }

    pub const ALL: [Self; 11] = [
        Self::Br,
        Self::Us,
        Self::Eu,
        Self::Uk,
        Self::Jp,
        Self::Kr,
        Self::Tw,
        Self::Hk,
        Self::Sg,
        Self::Cn,
        Self::Other,
    ];

    /// The app is offered to the public here.
    pub fn is_public(self) -> bool {
        !matches!(self, Self::Cn | Self::Other)
    }

    /// AI services must be ones that serve mainland China.
    pub fn is_mainland_china(self) -> bool {
        self == Self::Cn
    }

    /// A likely region for a UI language, to preselect; the student confirms.
    pub fn guess(locale: &str) -> Option<Self> {
        match locale {
            "pt-BR" => Some(Self::Br),
            "ja" => Some(Self::Jp),
            "ko" => Some(Self::Kr),
            "zh-Hant" => Some(Self::Tw),
            "zh-Hans" => Some(Self::Sg),
            _ => None,
        }
    }
}

/// When the student agreed to the terms of use and the privacy policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermsAcceptance {
    /// [`TERMS_VERSION`] at the time.
    pub version: String,
    /// Local time, `%Y-%m-%d %H:%M:%S`.
    pub accepted_at: String,
    /// The region whose terms were shown.
    #[serde(default)]
    pub region: Option<Region>,
}

/// Seconds of quiet that end the student's turn (see [`Settings::turn_pause`]).
pub const DEFAULT_TURN_PAUSE: f32 = 1.6;
pub const TURN_PAUSE_RANGE: std::ops::RangeInclusive<f32> = 0.5..=5.0;
/// Extra quiet a sentence without an ending gets (see [`Settings::long_pause`]).
const UNFINISHED_EXTRA: f32 = 1.5;
/// The default before 1 s cut students off mid-answer.
const OLD_DEFAULT_TURN_PAUSE: f32 = 1.0;
/// Bumped when saved turn pauses are moved to a new default.
const TURN_PAUSE_VERSION: u32 = 1;
/// Version 1, only in development builds, turned the English voice on for
/// everyone; its sudden change of voice sounded wrong, and the main voice now
/// says English terms from the tutor's respelling instead. Version 2 turns
/// off what version 1 turned on; a voice chosen by the student stays.
const ENGLISH_VOICE_VERSION: u32 = 2;

use crate::Teacher;

/// How photos of handwritten answers are transcribed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptOcr {
    /// Run PaddleOCR-VL on this computer; photos remain local.
    #[default]
    Local,
    /// Send photos to the configured vision model (never available with Copilot).
    ExternalVision,
}

/// Default volume of the interface sounds.
pub const DEFAULT_UI_SOUND_VOLUME: f32 = 0.6;

/// Default time between study reminders: three hours.
pub const DEFAULT_REMINDER_MINUTES: u32 = 180;

/// Everything the user can change in Settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// How the tutor calls the student; empty when not given.
    pub student_name: String,
    pub ai: AiConfig,
    /// Transcription backend for photographed handwritten answers.
    pub manuscript_ocr: ManuscriptOcr,
    /// Read tutor answers aloud.
    pub voice_enabled: bool,
    /// Microphone by name; `None` follows the system default.
    pub input_device: Option<String>,
    /// Speakers or headphones by name; `None` follows the system default.
    pub output_device: Option<String>,
    /// The tutor notes how the student likes to learn and reads those notes
    /// at the start of each lesson (see `notes`).
    pub student_notes: bool,
    /// Stop listening when the student goes quiet.
    pub auto_stop: bool,
    /// Quiet, in seconds, after a finished sentence before the student's
    /// turn ends. Longer pauses mid-thought always wait a bit more.
    pub turn_pause: f32,
    /// Last [`TURN_PAUSE_VERSION`] applied; 0 in settings saved before it, so
    /// the old default is raised once and a pause the student chose stays.
    #[serde(default)]
    pub turn_pause_version: u32,
    /// The tutor says "Hum." or "Entendi." right away while the answer is
    /// being written.
    pub fillers: bool,
    /// Speech recognizer; falls back to whichever is installed.
    pub stt_engine: SttEngine,
    /// The student picked the recognizer in Settings. Until then it follows
    /// the computer ([`kg_audio::SpeechHardware::recommended_stt`]).
    pub stt_engine_chosen: bool,
    /// Whisper model, used when Whisper is the recognizer and to double-check
    /// one-word answers with Parakeet.
    pub whisper_model: WhisperModel,
    pub voice: Voice,
    /// Kokoro voice for English passages in lessons; `None` reads them in
    /// the main voice.
    pub english_voice: Option<EnglishVoice>,
    /// Last [`ENGLISH_VOICE_VERSION`] applied; 0 in settings saved before it.
    #[serde(default)]
    pub english_voice_version: u32,
    pub tts_backend: TtsBackend,
    pub cloud_speech: CloudSpeechConfig,
    pub speech_pitch: f32,
    /// Piper intonation variation (`noise_scale`).
    pub speech_expressiveness: f32,
    /// Piper rhythm variation (`noise_w`).
    pub speech_rhythm_variation: f32,
    /// Look up pictures for the lesson whiteboard on Portuguese Wikipedia
    /// (only the search term is sent).
    pub board_images: bool,
    /// Ask GitHub for a newer version when the app opens (only that request
    /// is sent).
    pub check_updates: bool,
    /// Short interface sounds: clicks, hover ticks, chimes for answers and
    /// finished work.
    pub ui_sounds: bool,
    /// Volume of the interface sounds, `0.0..=1.0`.
    pub ui_sound_volume: f32,
    /// Kinds of interface sound turned off one by one (e.g. only hover).
    pub muted_sound_groups: Vec<kg_audio::CueGroup>,
    /// Celebrations and small motion on answers (confetti, pulses).
    pub animations: bool,
    /// Show a study reminder while the student is away from the app.
    pub study_reminders: bool,
    /// Minutes between study reminders.
    pub reminder_minutes: u32,
    /// Who teaches; `None` in settings saved before teachers existed (see
    /// [`Settings::teacher`]).
    #[serde(default, deserialize_with = "crate::teacher::lenient")]
    pub teacher: Option<Teacher>,
    /// Speech speed multiplier: 1.0 is normal, 1.2 is 20% faster.
    pub speech_rate: f32,
    pub quiz_size: u32,
    /// Multiple choice or Certo/Errado, or as the exam asks.
    pub quiz_style: kg_study::QuizStyle,
    /// How long and deep lessons go.
    pub lesson_length: kg_ai::LessonLength,
    /// UI locale, e.g. "pt-BR".
    pub locale: String,
    /// The first-run welcome steps were closed.
    pub onboarding_done: bool,
    /// Last [`SETUP_VERSION`] completed; 0 in settings saved before it.
    pub setup_version: u32,
    pub terms_accepted: Option<TermsAcceptance>,
    /// Id of the AI service picked on setup (`kg_ai::PROVIDERS`); empty when
    /// typed by hand.
    pub ai_provider: String,
    /// The guided tour of the screens was finished or skipped.
    pub tour_done: bool,
    /// Study was imported from another computer and this one has no AI
    /// service yet: the first-run setup opens on the AI step.
    pub ai_setup_pending: bool,
    /// Where the app is used; chosen with the terms.
    pub region: Option<Region>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            student_name: String::new(),
            ai: AiConfig::default(),
            manuscript_ocr: ManuscriptOcr::default(),
            voice_enabled: true,
            input_device: None,
            output_device: None,
            student_notes: true,
            auto_stop: true,
            turn_pause: DEFAULT_TURN_PAUSE,
            turn_pause_version: TURN_PAUSE_VERSION,
            fillers: true,
            stt_engine: SttEngine::default(),
            stt_engine_chosen: false,
            whisper_model: WhisperModel::default(),
            voice: Voice::default(),
            english_voice: None,
            english_voice_version: ENGLISH_VOICE_VERSION,
            tts_backend: TtsBackend::default(),
            cloud_speech: CloudSpeechConfig::default(),
            speech_pitch: 0.0,
            speech_expressiveness: kg_audio::DEFAULT_EXPRESSIVENESS,
            speech_rhythm_variation: kg_audio::DEFAULT_RHYTHM_VARIATION,
            board_images: true,
            check_updates: true,
            ui_sounds: true,
            ui_sound_volume: DEFAULT_UI_SOUND_VOLUME,
            muted_sound_groups: Vec::new(),
            animations: true,
            study_reminders: false,
            reminder_minutes: DEFAULT_REMINDER_MINUTES,
            teacher: None,
            speech_rate: 0.95,
            quiz_size: 5,
            quiz_style: kg_study::QuizStyle::default(),
            lesson_length: kg_ai::LessonLength::default(),
            locale: "pt-BR".into(),
            onboarding_done: false,
            setup_version: 0,
            terms_accepted: None,
            ai_provider: String::new(),
            tour_done: false,
            ai_setup_pending: false,
            region: None,
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
        if settings.ai.api_key.is_empty() && same_server(&settings.ai, env_ai) {
            settings.ai.api_key = env_ai.api_key.clone();
        }
        settings.normalize();
        settings.follow_hardware(kg_audio::SpeechHardware::detect().recommended_stt());
        Ok(settings)
    }

    /// Until the student picks a recognizer, use the one for this computer:
    /// Nemotron with 8 GB of memory or more, Parakeet otherwise. Whisper
    /// was never a default, so it was picked on purpose.
    fn follow_hardware(&mut self, recommended: SttEngine) {
        if !self.stt_engine_chosen && self.stt_engine != SttEngine::Whisper {
            self.stt_engine = recommended;
        }
    }

    /// The required setup is missing, older than [`SETUP_VERSION`], or the
    /// terms changed since the student agreed.
    pub fn needs_setup(&self) -> bool {
        self.setup_version < SETUP_VERSION
            || self.ai_setup_pending
            || self.region.is_none()
            || self
                .terms_accepted
                .as_ref()
                .is_none_or(|t| t.version != TERMS_VERSION || t.region != self.region)
    }

    /// The chosen teacher, or one matching the voice's gender.
    pub fn teacher(&self) -> Teacher {
        self.teacher
            .unwrap_or_else(|| Teacher::for_gender(self.voice_gender()))
    }

    /// Gender of the voice that speaks the lessons.
    pub fn voice_gender(&self) -> VoiceGender {
        match self.tts_backend {
            TtsBackend::Cloud => self.cloud_speech.gender,
            TtsBackend::Local => self.voice.gender(),
        }
    }

    /// Quiet after a finished sentence that ends the student's turn.
    pub(crate) fn sentence_pause(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f32(self.turn_pause)
    }

    /// Quiet that ends the turn without a finished sentence: a pause to
    /// think of a word should not cut the student off.
    pub(crate) fn long_pause(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f32(self.turn_pause + UNFINISHED_EXTRA)
    }

    pub(crate) fn normalize(&mut self) {
        self.quiz_size = self.quiz_size.clamp(3, 10);
        if self.english_voice_version < ENGLISH_VOICE_VERSION {
            if self.english_voice_version == 1 {
                self.english_voice = None;
            }
            self.english_voice_version = ENGLISH_VOICE_VERSION;
        }
        if self.turn_pause_version < TURN_PAUSE_VERSION {
            if (self.turn_pause - OLD_DEFAULT_TURN_PAUSE).abs() < 0.05 {
                self.turn_pause = DEFAULT_TURN_PAUSE;
            }
            self.turn_pause_version = TURN_PAUSE_VERSION;
        }
        self.turn_pause = if self.turn_pause.is_finite() {
            self.turn_pause
                .clamp(*TURN_PAUSE_RANGE.start(), *TURN_PAUSE_RANGE.end())
        } else {
            DEFAULT_TURN_PAUSE
        };
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

/// True when `ai` points at the server the environment configures. The
/// `OPENAI_API_KEY` from `.env` or the system belongs to that server only:
/// sending it to another one (a LiteLLM key to Z.ai, say) is always refused.
pub(crate) fn same_server(ai: &AiConfig, env_ai: &AiConfig) -> bool {
    let trim = |url: &str| url.trim().trim_end_matches('/').to_ascii_lowercase();
    trim(&ai.base_url) == trim(&env_ai.base_url)
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
    fn english_voice_turned_on_by_version_one_is_turned_off_once() {
        let mut turned_on: Settings =
            serde_json::from_str(r#"{"english_voice":"af_heart","english_voice_version":1}"#)
                .unwrap();
        turned_on.normalize();
        assert_eq!(turned_on.english_voice, None);
        assert_eq!(turned_on.english_voice_version, ENGLISH_VOICE_VERSION);
        // Chosen again afterwards, it stays.
        turned_on.english_voice = Some(EnglishVoice::Heart);
        turned_on.normalize();
        assert_eq!(turned_on.english_voice, Some(EnglishVoice::Heart));

        let mut chosen: Settings =
            serde_json::from_str(r#"{"english_voice":"am_michael"}"#).unwrap();
        chosen.normalize();
        assert_eq!(chosen.english_voice, Some(EnglishVoice::Michael));
    }

    #[test]
    fn old_default_turn_pause_is_raised_once_and_a_chosen_one_stays() {
        let mut old: Settings = serde_json::from_str(r#"{"turn_pause":1.0}"#).unwrap();
        assert_eq!(old.turn_pause_version, 0);
        old.normalize();
        assert_eq!(old.turn_pause, DEFAULT_TURN_PAUSE);
        assert_eq!(old.turn_pause_version, TURN_PAUSE_VERSION);

        let mut chosen: Settings = serde_json::from_str(r#"{"turn_pause":2.4}"#).unwrap();
        chosen.normalize();
        assert_eq!(chosen.turn_pause, 2.4);

        // Once moved, picking 1 s again is respected.
        let mut again = old.clone();
        again.turn_pause = 1.0;
        again.normalize();
        assert_eq!(again.turn_pause, 1.0);
    }

    #[test]
    fn recognizer_follows_the_computer_until_picked() {
        let old: Settings = serde_json::from_str(r#"{"stt_engine":"parakeet"}"#).unwrap();
        assert!(!old.stt_engine_chosen);
        let mut settings = old.clone();
        settings.follow_hardware(SttEngine::Nemotron);
        assert_eq!(settings.stt_engine, SttEngine::Nemotron);
        settings.follow_hardware(SttEngine::Parakeet);
        assert_eq!(settings.stt_engine, SttEngine::Parakeet);

        let picked = Settings {
            stt_engine: SttEngine::Parakeet,
            stt_engine_chosen: true,
            ..Settings::default()
        };
        let mut kept = picked.clone();
        kept.follow_hardware(SttEngine::Nemotron);
        assert_eq!(kept, picked);

        let mut whisper: Settings = serde_json::from_str(r#"{"stt_engine":"whisper"}"#).unwrap();
        whisper.follow_hardware(SttEngine::Nemotron);
        assert_eq!(whisper.stt_engine, SttEngine::Whisper);
    }

    #[test]
    fn settings_from_before_the_setup_need_it() {
        let old = r#"{"student_name":"Lucas","onboarding_done":true}"#;
        let settings: Settings = serde_json::from_str(old).unwrap();
        assert_eq!(settings.student_name, "Lucas");
        assert_eq!(settings.setup_version, 0);
        assert!(!settings.study_reminders);
        assert!(settings.needs_setup());
        let done = Settings {
            setup_version: SETUP_VERSION,
            terms_accepted: Some(TermsAcceptance {
                version: TERMS_VERSION.into(),
                accepted_at: "2026-10-30 20:00:00".into(),
                region: Some(Region::Jp),
            }),
            region: Some(Region::Jp),
            ..Settings::default()
        };
        assert!(!done.needs_setup());
        // Moving to another region asks for its terms.
        let moved = Settings {
            region: Some(Region::Tw),
            ..done.clone()
        };
        assert!(moved.needs_setup());
        // Agreement to an older version (before regions) asks again.
        let old: TermsAcceptance =
            serde_json::from_str(r#"{"version":"2026-10-29","accepted_at":"x"}"#).unwrap();
        assert_eq!(old.region, None);
    }

    #[test]
    fn first_run_uses_env() {
        let store = Store::open_in_memory().unwrap();
        let settings = Settings::load(&store, &env_ai()).unwrap();
        assert_eq!(settings.ai, env_ai());
        assert!(settings.voice_enabled);
    }

    #[test]
    fn manuscript_ocr_defaults_to_local_for_new_and_existing_settings() {
        let current = Settings::default();
        let existing: Settings =
            serde_json::from_str(r#"{"student_name":"Lucas","onboarding_done":true}"#).unwrap();

        assert_eq!(current.manuscript_ocr, ManuscriptOcr::Local);
        assert_eq!(existing.manuscript_ocr, ManuscriptOcr::Local);
    }

    #[test]
    fn external_vision_choice_survives_settings_serialization() {
        let settings = Settings {
            manuscript_ocr: ManuscriptOcr::ExternalVision,
            ..Settings::default()
        };

        let serialized = serde_json::to_string(&settings).unwrap();
        let restored: Settings = serde_json::from_str(&serialized).unwrap();

        assert_eq!(restored.manuscript_ocr, ManuscriptOcr::ExternalVision);
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
        // The `.env` key belongs to the `.env` server, not to this one.
        assert_eq!(loaded.ai.api_key, "");

        settings.ai.api_key = "typed-key".into();
        settings.save(&mut store, &env_ai()).unwrap();
        assert_eq!(
            Settings::load(&store, &env_ai()).unwrap().ai.api_key,
            "typed-key"
        );
    }

    #[test]
    fn env_key_is_never_sent_to_another_server() {
        let mut store = Store::open_in_memory().unwrap();
        let mut settings = Settings::load(&store, &env_ai()).unwrap();
        assert_eq!(settings.ai.api_key, "env-key");
        settings.ai.base_url = "https://api.z.ai/api/paas/v4".into();
        settings.save(&mut store, &env_ai()).unwrap();
        assert_eq!(Settings::load(&store, &env_ai()).unwrap().ai.api_key, "");

        // Back on the `.env` server (any spelling), its key applies again.
        settings.ai.base_url = "https://API.example/v4/ ".into();
        settings.save(&mut store, &env_ai()).unwrap();
        assert_eq!(
            Settings::load(&store, &env_ai()).unwrap().ai.api_key,
            "env-key"
        );
    }
}
