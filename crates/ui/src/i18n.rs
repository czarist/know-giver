//! Locale selection and translated labels for engine types.
//!
//! Translations live in `locales/<locale>.yml` at the workspace root and are
//! embedded at compile time. pt-BR is the default and the fallback.

use chrono::NaiveDate;
use kg_engine::{
    AiError, AudioError, Difficulty, EngineError, HandoutError, LessonPhase, ModelId, Notice,
    PdfError, PiperVoice, ReviewGrade, StoreError, StudyError, TopicStatus, WhisperModel,
};
use rust_i18n::t;

/// Locale used on first start and for any missing key.
pub const DEFAULT_LOCALE: &str = "pt-BR";

/// Locales offered in Settings, in display order.
pub const LOCALES: [&str; 2] = ["pt-BR", "en"];

pub fn current() -> String {
    rust_i18n::locale().to_string()
}

/// Switches the UI language; unknown locales fall back to the default.
pub fn set(locale: &str) {
    let locale = if LOCALES.contains(&locale) {
        locale
    } else {
        DEFAULT_LOCALE
    };
    rust_i18n::set_locale(locale);
}

/// The language's own name (e.g. "English"), for the language picker.
pub fn language_name(locale: &str) -> String {
    t!(format!("language.{locale}")).into_owned()
}

pub fn date(date: NaiveDate) -> String {
    date.format(&t!("format.date")).to_string()
}

pub fn status(status: TopicStatus) -> String {
    match status {
        TopicStatus::NotStarted => t!("status.not_started"),
        TopicStatus::Studying => t!("status.studying"),
        TopicStatus::Reviewing => t!("status.reviewing"),
        TopicStatus::Done => t!("status.done"),
    }
    .into_owned()
}

pub fn difficulty(difficulty: Difficulty) -> String {
    match difficulty {
        Difficulty::Easy => t!("difficulty.easy"),
        Difficulty::Medium => t!("difficulty.medium"),
        Difficulty::Hard => t!("difficulty.hard"),
    }
    .into_owned()
}

pub fn grade(grade: ReviewGrade) -> String {
    match grade {
        ReviewGrade::Again => t!("grade.again"),
        ReviewGrade::Hard => t!("grade.hard"),
        ReviewGrade::Good => t!("grade.good"),
        ReviewGrade::Easy => t!("grade.easy"),
    }
    .into_owned()
}

pub fn phase(phase: LessonPhase) -> String {
    match phase {
        LessonPhase::Idle => t!("lesson.phase.idle"),
        LessonPhase::Listening => t!("lesson.phase.listening"),
        LessonPhase::Transcribing => t!("lesson.phase.transcribing"),
        LessonPhase::Thinking => t!("lesson.phase.thinking"),
        LessonPhase::Speaking => t!("lesson.phase.speaking"),
    }
    .into_owned()
}

pub fn whisper_model(model: WhisperModel) -> String {
    match model {
        WhisperModel::Tiny => t!("models.whisper.tiny"),
        WhisperModel::Base => t!("models.whisper.base"),
        WhisperModel::Small => t!("models.whisper.small"),
    }
    .into_owned()
}

pub fn voice(voice: PiperVoice) -> String {
    match voice {
        PiperVoice::Faber => t!("models.voice.faber"),
        PiperVoice::Cadu => t!("models.voice.cadu"),
        PiperVoice::Jeff => t!("models.voice.jeff"),
    }
    .into_owned()
}

pub fn model_name(id: ModelId) -> String {
    match id {
        ModelId::Whisper(m) => whisper_model(m),
        ModelId::Voice(v) => voice(v),
        ModelId::PiperRuntime => t!("models.piper").into_owned(),
    }
}

pub fn model_description(id: ModelId) -> String {
    match id {
        ModelId::Whisper(_) => t!("models.whisper.description"),
        ModelId::Voice(_) => t!("models.voice.description"),
        ModelId::PiperRuntime => t!("models.piper_description"),
    }
    .into_owned()
}

/// Text for a notice, and whether it is an error.
pub fn notice(notice: &Notice) -> (String, bool) {
    let text = match notice {
        Notice::Error(err) => return (error_message(err), true),
        Notice::PlanCreated => t!("notice.plan_created"),
        Notice::SettingsSaved => t!("notice.settings_saved"),
        Notice::ConnectionOk => t!("notice.connection_ok"),
        Notice::ModelInstalled(id) => t!("notice.model_installed", model = model_name(*id)),
        Notice::LessonSaved => t!("notice.lesson_saved"),
        Notice::ReviewSaved {
            next_due: Some(due),
        } => {
            t!("notice.review_saved", date = date(*due))
        }
        Notice::ReviewSaved { next_due: None } => t!("notice.review_saved_no_date"),
        Notice::QuizSaved { correct, total } => {
            t!("notice.quiz_saved", correct = correct, total = total)
        }
        Notice::NothingHeard => t!("notice.nothing_heard"),
        Notice::Exported(path) => t!("notice.exported", path = path.display()),
        Notice::HandoutSaved(path) => t!("notice.handout_saved", path = path.display()),
    };
    (text.into_owned(), false)
}

/// User-facing text for an engine error, in the current locale.
/// Technical details stay in the error's `Display`, which goes to the log.
pub fn error_message(err: &EngineError) -> String {
    let text = match err {
        EngineError::Study(StudyError::EmptyPlan) => t!("error.study.empty_plan"),
        EngineError::Study(StudyError::EmptySubject(subject)) => {
            t!("error.study.empty_subject", subject = subject)
        }
        EngineError::Study(StudyError::EmptyQuiz | StudyError::InvalidQuestion(_)) => {
            t!("error.study.bad_quiz")
        }
        EngineError::Store(StoreError::PlanNotFound(_)) => t!("error.store.not_found"),
        EngineError::Store(_) => t!("error.store.generic"),
        EngineError::Ai(AiError::Connection { url, .. }) => t!("error.ai.connection", url = url),
        EngineError::Ai(AiError::Auth { .. }) => t!("error.ai.auth"),
        EngineError::Ai(AiError::RateLimited) => t!("error.ai.rate_limited"),
        EngineError::Ai(AiError::Http { status, .. }) => t!("error.ai.http", status = status),
        EngineError::Ai(AiError::InvalidResponse(_)) => t!("error.ai.invalid_response"),
        EngineError::Audio(AudioError::NoInputDevice) => t!("error.audio.no_input_device"),
        EngineError::Audio(AudioError::NoOutputDevice(_)) => t!("error.audio.no_output_device"),
        EngineError::Audio(AudioError::Device(_)) => t!("error.audio.device"),
        EngineError::Audio(AudioError::ModelMissing(_)) => t!("error.audio.model_missing"),
        EngineError::Audio(AudioError::Model(_)) => t!("error.audio.model"),
        EngineError::Audio(AudioError::Tts(_)) => t!("error.audio.tts"),
        EngineError::Audio(AudioError::Download { .. }) => t!("error.audio.download"),
        EngineError::Audio(AudioError::Cancelled) => t!("error.audio.cancelled"),
        EngineError::Audio(AudioError::Io { path, .. }) => {
            t!("error.audio.io", path = path.display())
        }
        EngineError::Audio(AudioError::Unsupported(_)) => t!("error.audio.unsupported"),
        EngineError::Pdf(PdfError::Io { path, .. }) => t!("error.pdf.io", path = path.display()),
        EngineError::Pdf(PdfError::Parse { path, .. }) => {
            t!("error.pdf.parse", path = path.display())
        }
        EngineError::Pdf(PdfError::NoText(path)) => t!("error.pdf.no_text", path = path.display()),
        EngineError::Export { path, .. } => t!("error.export", path = path.display()),
        EngineError::Handout(HandoutError::Empty) => t!("error.handout.empty"),
        EngineError::Handout(_) => t!("error.handout.render"),
        EngineError::Cancelled => t!("error.cancelled"),
        EngineError::NoEdital => t!("error.no_edital"),
        EngineError::NoActivePlan => t!("error.no_active_plan"),
        EngineError::TopicNotFound(_) => t!("error.topic_not_found"),
        EngineError::SpeechModelMissing(id) => {
            t!("error.speech_model_missing", model = model_name(*id))
        }
    };
    text.into_owned()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn keys(locale: &str) -> BTreeSet<String> {
        crate::_rust_i18n_backend()
            .messages_for_locale(locale)
            .unwrap_or_default()
            .into_iter()
            .map(|(key, _)| key.into_owned())
            .collect()
    }

    #[test]
    fn every_offered_locale_has_the_same_keys() {
        let reference = keys(DEFAULT_LOCALE);
        assert!(!reference.is_empty(), "{DEFAULT_LOCALE} has no keys");
        for locale in LOCALES {
            let other = keys(locale);
            let missing: Vec<_> = reference.difference(&other).collect();
            let extra: Vec<_> = other.difference(&reference).collect();
            assert!(
                missing.is_empty() && extra.is_empty(),
                "{locale}: missing {missing:?}, extra {extra:?}"
            );
        }
    }

    /// Catches typos in `t!("...")` keys, which would otherwise show the raw
    /// key on screen.
    #[test]
    fn every_key_used_in_code_exists() {
        let sources = [
            include_str!("lib.rs"),
            include_str!("app.rs"),
            include_str!("i18n.rs"),
            include_str!("widgets.rs"),
            include_str!("export.rs"),
            include_str!("screens/mod.rs"),
            include_str!("screens/edital.rs"),
            include_str!("screens/handout.rs"),
            include_str!("screens/plan.rs"),
            include_str!("screens/lesson.rs"),
            include_str!("screens/quiz.rs"),
            include_str!("screens/review.rs"),
            include_str!("screens/progress.rs"),
            include_str!("screens/settings.rs"),
        ];
        let known = keys(DEFAULT_LOCALE);
        let mut unknown = Vec::new();
        for source in sources {
            for chunk in source.split("t!(").skip(1) {
                // The key may be on the next line after `t!(`.
                let Some(rest) = chunk.trim_start().strip_prefix('"') else {
                    continue;
                };
                let key = rest.split('"').next().unwrap_or_default();
                // Only literal keys; dynamic ones (`format!`) have their own tests.
                let is_literal_key = key.starts_with(|c: char| c.is_ascii_lowercase())
                    && key
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
                if is_literal_key && !known.contains(key) {
                    unknown.push(key.to_owned());
                }
            }
        }
        assert!(unknown.is_empty(), "keys missing from locales: {unknown:?}");
    }

    #[test]
    fn every_offered_locale_has_a_language_name() {
        let known = keys(DEFAULT_LOCALE);
        for locale in LOCALES {
            assert!(known.contains(&format!("language.{locale}")), "{locale}");
        }
    }

    #[test]
    fn date_format_is_translated() {
        set("pt-BR");
        let d = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap_or_default();
        assert_eq!(date(d), "09/03/2026");
    }
}
