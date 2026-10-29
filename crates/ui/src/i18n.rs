//! Locale selection and translated labels for engine types.
//!
//! Translations live in `locales/<locale>.yml` at the workspace root and are
//! embedded at compile time. pt-BR is the default and the fallback.

use chrono::NaiveDate;
use kg_engine::{
    AiError, AttemptStatus, AudioError, Criterion, Difficulty, EngineError, HandoutError,
    ImageProblem, KokoroVoice, LessonPhase, MAX_PAGES, Milestone, ModelId, Notice, PageIssue,
    PdfError, PiperVoice, ProblemCategory, ReviewGrade, RubricError, RubricSource, StoreError,
    StudyError, TopicStatus, Voice, VoiceGender, WhisperModel, WritingKind,
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

pub fn voice(voice: Voice) -> String {
    match voice {
        Voice::Piper(PiperVoice::Faber) => t!("models.voice.faber"),
        Voice::Piper(PiperVoice::Cadu) => t!("models.voice.cadu"),
        Voice::Piper(PiperVoice::Jeff) => t!("models.voice.jeff"),
        Voice::Kokoro(KokoroVoice::Dora) => t!("models.voice.dora"),
        Voice::Kokoro(KokoroVoice::Alex) => t!("models.voice.alex"),
        Voice::Kokoro(KokoroVoice::Santa) => t!("models.voice.santa"),
        Voice::Piper(PiperVoice::Dii) => t!("models.voice.dii"),
        Voice::Kokoro(KokoroVoice::Sara) => t!("models.voice.sara"),
        Voice::Kokoro(KokoroVoice::Heart) => t!("models.voice.heart"),
        Voice::Kokoro(KokoroVoice::Emma) => t!("models.voice.emma"),
    }
    .into_owned()
}

pub fn teacher_name(teacher: kg_engine::Teacher) -> String {
    use kg_engine::Teacher;
    match teacher {
        Teacher::Helena => t!("teachers.helena.name"),
        Teacher::Lia => t!("teachers.lia.name"),
        Teacher::Rafael => t!("teachers.rafael.name"),
        Teacher::Otavio => t!("teachers.otavio.name"),
    }
    .into_owned()
}

pub fn teacher_about(teacher: kg_engine::Teacher) -> String {
    use kg_engine::Teacher;
    match teacher {
        Teacher::Helena => t!("teachers.helena.about"),
        Teacher::Lia => t!("teachers.lia.about"),
        Teacher::Rafael => t!("teachers.rafael.about"),
        Teacher::Otavio => t!("teachers.otavio.about"),
    }
    .into_owned()
}

pub fn voice_gender(gender: VoiceGender) -> String {
    match gender {
        VoiceGender::Female => t!("settings.voice.female"),
        VoiceGender::Male => t!("settings.voice.male"),
    }
    .into_owned()
}

pub fn model_name(id: ModelId) -> String {
    match id {
        ModelId::Whisper(m) => whisper_model(m),
        ModelId::Voice(v) => voice(Voice::Piper(v)),
        ModelId::PiperRuntime => t!("models.piper").into_owned(),
        ModelId::KokoroRuntime => t!("models.kokoro_runtime").into_owned(),
        ModelId::KokoroModel => t!("models.kokoro_model").into_owned(),
        ModelId::Parakeet => t!("models.parakeet").into_owned(),
    }
}

pub fn model_description(id: ModelId) -> String {
    match id {
        ModelId::Whisper(_) => t!("models.whisper.description"),
        ModelId::Voice(PiperVoice::Dii) => t!("models.voice.dii_description"),
        ModelId::Voice(_) => t!("models.voice.description"),
        ModelId::PiperRuntime => t!("models.piper_description"),
        ModelId::KokoroRuntime => t!("models.kokoro_runtime_description"),
        ModelId::KokoroModel => t!("models.kokoro_model_description"),
        ModelId::Parakeet => t!("models.parakeet_description"),
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
        Notice::ContentPrepared(summary) => match (&summary.pdf, summary.failed) {
            (Some(path), 0) => t!("notice.content_prepared", path = path.display()),
            (Some(path), failed) => t!(
                "notice.content_prepared_partial",
                path = path.display(),
                failed = failed
            ),
            (None, _) => return (t!("notice.content_prepare_failed").into_owned(), true),
        },
        Notice::BoardImported {
            used, set_aside, ..
        } => t!("notice.board_imported", used = used, set_aside = set_aside),
        Notice::AnswerKeyApplied { count } => t!("notice.answer_key", count = count),
        Notice::Milestone(Milestone::Streak(days)) => t!("notice.milestone_streak", days = days),
        Notice::Milestone(Milestone::Progress(percent)) => {
            t!("notice.milestone_progress", percent = percent)
        }
        Notice::ProposalCreated => t!("notice.proposal_created"),
        Notice::EditalRevalidated { written_test: true } => {
            t!("notice.edital_revalidated_writing")
        }
        Notice::EditalRevalidated {
            written_test: false,
        } => t!("notice.edital_revalidated"),
        Notice::WritingEvaluated { total, max } => t!(
            "notice.writing_evaluated",
            total = points(*total),
            max = points(*max)
        ),
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
        EngineError::Ai(AiError::NoVisionModel) => t!("error.ai.no_vision_model"),
        EngineError::Ai(AiError::VisionRejected { status }) => {
            t!("error.ai.vision_rejected", status = status)
        }
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
        EngineError::Image(ImageProblem::TooLarge) => t!("error.image.too_large"),
        EngineError::Image(ImageProblem::Unsupported) => t!("error.image.unsupported"),
        EngineError::Image(ImageProblem::TooManyPixels) => t!("error.image.too_many_pixels"),
        EngineError::Image(ImageProblem::Unreadable) => t!("error.image.unreadable"),
        EngineError::NoWrittenTest => t!("error.writing.no_written_test"),
        EngineError::TooManyPages => t!("error.writing.too_many_pages", max = MAX_PAGES),
        EngineError::NoPages => t!("error.writing.no_pages"),
        EngineError::PageNeedsRetake => t!("error.writing.page_needs_retake"),
        EngineError::AttemptLocked => t!("error.writing.attempt_locked"),
        EngineError::LooksLikeRewrite => t!("error.writing.looks_like_rewrite"),
        EngineError::NotEnoughAttempts => t!("error.writing.not_enough_attempts"),
        EngineError::Rubric(RubricError::BadLine(line)) => {
            t!("error.writing.rubric_line", line = line)
        }
        EngineError::Rubric(_) => t!("error.writing.rubric"),
    };
    text.into_owned()
}

/// Points without needless decimals ("7", "7.5").
pub fn points(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    let text = if (rounded - rounded.trunc()).abs() < f64::EPSILON {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    };
    text.replace('.', &t!("format.decimal"))
}

pub fn writing_kind(kind: WritingKind) -> String {
    match kind {
        WritingKind::Essay => t!("discursive.kind.essay"),
        WritingKind::Technical => t!("discursive.kind.technical"),
        WritingKind::CaseStudy => t!("discursive.kind.case"),
    }
    .into_owned()
}

pub fn attempt_status(status: AttemptStatus) -> String {
    match status {
        AttemptStatus::Draft => t!("discursive.status.draft"),
        AttemptStatus::Transcribing => t!("discursive.status.transcribing"),
        AttemptStatus::Retake => t!("discursive.status.retake"),
        AttemptStatus::Review => t!("discursive.status.review"),
        AttemptStatus::Evaluating => t!("discursive.status.evaluating"),
        AttemptStatus::Evaluated => t!("discursive.status.evaluated"),
        AttemptStatus::Pending => t!("discursive.status.pending"),
        AttemptStatus::Failed => t!("discursive.status.failed"),
    }
    .into_owned()
}

pub fn page_issue(issue: PageIssue) -> String {
    match issue {
        PageIssue::Dark => t!("discursive.issue.dark"),
        PageIssue::Overexposed => t!("discursive.issue.overexposed"),
        PageIssue::Blurry => t!("discursive.issue.blurry"),
        PageIssue::LowContrast => t!("discursive.issue.low_contrast"),
        PageIssue::LowResolution => t!("discursive.issue.low_resolution"),
        PageIssue::Cropped => t!("discursive.issue.cropped"),
        PageIssue::Illegible => t!("discursive.issue.illegible"),
    }
    .into_owned()
}

pub fn problem_category(category: ProblemCategory) -> String {
    match category {
        ProblemCategory::Writing => t!("discursive.category.writing"),
        ProblemCategory::Content => t!("discursive.category.content"),
        ProblemCategory::Structure => t!("discursive.category.structure"),
    }
    .into_owned()
}

pub fn rubric_source(source: RubricSource) -> String {
    match source {
        RubricSource::Edital => t!("discursive.rubric.edital"),
        RubricSource::Manual => t!("discursive.rubric.manual"),
        RubricSource::Generic => t!("discursive.rubric.generic"),
    }
    .into_owned()
}

/// Locale key of one of Know Giver's generic criteria.
fn generic_key(criterion: &Criterion) -> Option<String> {
    criterion
        .key
        .strip_prefix("generic.")
        .map(|rest| format!("discursive.criteria.{}", rest.replace('.', "_")))
}

/// A criterion's name: generic ones are translated, others shown as given.
pub fn criterion_name(criterion: &Criterion) -> String {
    match generic_key(criterion) {
        Some(key) => t!(key).into_owned(),
        None => criterion.name.clone(),
    }
}

pub fn criterion_description(criterion: &Criterion) -> String {
    match generic_key(criterion) {
        Some(key) => t!(format!("{key}_hint")).into_owned(),
        None => criterion.description.clone(),
    }
}

/// Name of a lesson length.
pub fn lesson_length(length: kg_engine::LessonLength) -> String {
    use kg_engine::LessonLength;
    match length {
        LessonLength::Quick => t!("settings.lesson_length.quick"),
        LessonLength::Normal => t!("settings.lesson_length.normal"),
        LessonLength::Full => t!("settings.lesson_length.full"),
    }
    .into_owned()
}

/// What a lesson length means.
/// Name of a voice for English passages, with its accent.
pub fn english_voice(voice: kg_engine::EnglishVoice) -> String {
    use kg_engine::EnglishVoice as V;
    match voice {
        V::Heart => t!("settings.english_voice.heart"),
        V::Michael => t!("settings.english_voice.michael"),
        V::Emma => t!("settings.english_voice.emma"),
        V::George => t!("settings.english_voice.george"),
    }
    .into_owned()
}

pub fn lesson_length_hint(length: kg_engine::LessonLength) -> String {
    use kg_engine::LessonLength;
    match length {
        LessonLength::Quick => t!("settings.lesson_length.quick_hint"),
        LessonLength::Normal => t!("settings.lesson_length.normal_hint"),
        LessonLength::Full => t!("settings.lesson_length.full_hint"),
    }
    .into_owned()
}

/// Name of a highlighter color.
pub fn highlight_color(color: kg_engine::HighlightColor) -> String {
    use kg_engine::HighlightColor;
    match color {
        HighlightColor::Yellow => t!("reader.color.yellow"),
        HighlightColor::Green => t!("reader.color.green"),
        HighlightColor::Blue => t!("reader.color.blue"),
        HighlightColor::Pink => t!("reader.color.pink"),
    }
    .into_owned()
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
            include_str!("screens/board.rs"),
            include_str!("screens/discursive.rs"),
            include_str!("screens/edital.rs"),
            include_str!("screens/handout.rs"),
            include_str!("screens/plan.rs"),
            include_str!("screens/lesson.rs"),
            include_str!("screens/quiz.rs"),
            include_str!("screens/review.rs"),
            include_str!("screens/progress.rs"),
            include_str!("screens/settings.rs"),
            include_str!("screens/today.rs"),
            include_str!("screens/welcome.rs"),
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
    fn generic_criteria_are_translated() {
        let known = keys(DEFAULT_LOCALE);
        for kind in WritingKind::ALL {
            for criterion in kg_study::discursive::generic_rubric(kind).criteria {
                let key = generic_key(&criterion).unwrap();
                assert!(known.contains(&key), "{key}");
                assert!(known.contains(&format!("{key}_hint")), "{key}_hint");
            }
        }
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
