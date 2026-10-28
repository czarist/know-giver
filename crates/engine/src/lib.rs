//! Engine: application state and orchestration between study, AI and audio.
//!
//! The UI reads state from [`Engine`] and sends it [`Command`]s. It never talks
//! to the AI, audio, PDF or storage layers directly. Slow work (PDF parsing,
//! model calls, speech) runs on worker threads; results come back as events
//! that [`Engine::poll`] applies on the UI thread.

mod board;
mod documents;
mod handouts;
mod lesson;
mod models;
mod paths;
mod plans;
mod practice;
mod prepare;
mod progress;
mod settings;
mod study_time;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use chrono::NaiveDate;

pub use board::{BoardItem, BoardJob};
pub use handouts::{HandoutJob, HandoutRequest, HandoutScope};
pub use kg_ai::{AiConfig, AiError, ModelSet, PlanPrefs, ReviewFeedback};
pub use kg_audio::{
    AudioError, CloudSpeechConfig, KokoroVoice, ModelId, PiperVoice, TtsBackend, Voice,
    VoiceGender, WhisperModel,
};
pub use kg_audio::{EXPRESSIVENESS_RANGE, RHYTHM_VARIATION_RANGE};
pub use kg_handout::{HandoutError, Labels as HandoutLabels};
pub use kg_pdf::{Edital, EditalText, PdfError};
pub use kg_study::{
    BoardDocument, BoardExcerpt, BoardQuestion, DocumentId, DocumentKind, Relevance,
};
pub use kg_study::{
    Difficulty, PlanId, PlanSummary, QuizQuestion, QuizStats, ReviewGrade, StoreError, StudyError,
    StudyPlan, Subject, SubjectId, Topic, TopicId, TopicStatus,
};
pub use lesson::{LessonPhase, LessonSession, LessonTurn, Speaker};
pub use models::ModelStatus;
pub use paths::AppPaths;
pub use practice::{QuizSession, QuizStep, ReviewSession, ReviewStep};
pub use prepare::{PrepareJob, PrepareRequest, PrepareSummary, slug};
pub use progress::Milestone;
pub use settings::{AvatarStyle, Settings};
pub use study_time::StudyPresence;

use kg_audio::{ModelStore, WhisperStt};
use kg_study::Store;

/// Requests from the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    ImportEdital(PathBuf),
    ClearEdital,
    GeneratePlan(PlanPrefs),
    SelectPlan(PlanId),
    DeletePlan(PlanId),
    SetTopicStatus(TopicId, TopicStatus),

    StartLesson(TopicId),
    EndLesson,
    StartListening,
    StopListening,
    SendMessage(String),
    StopSpeaking,
    PreviewVoice {
        settings: Settings,
        text: String,
    },
    StopVoicePreview,

    StartQuiz(TopicId),
    AnswerQuiz(usize),
    NextQuestion,
    CloseQuiz,

    StartReview(TopicId),
    SubmitReviewAnswer(String),
    GradeReview(ReviewGrade),
    CloseReview,

    SaveSettings(Settings),
    TestConnection,
    DownloadModel(ModelId),
    CancelDownload(ModelId),
    RemoveModel(ModelId),

    /// Closes the first-run welcome steps for good.
    FinishOnboarding,

    ExportHandout(HandoutRequest),
    CancelHandout,
    /// Writes handouts and quiz questions for every topic of the active plan,
    /// then saves the plan PDF. Sent while a plan is being generated, it runs
    /// as soon as that plan is ready.
    PrepareContent(PrepareRequest),
    CancelPrepare,

    /// Upload a past exam or board material for the active plan.
    ImportBoard {
        path: PathBuf,
        kind: DocumentKind,
    },
    /// Upload the answer key of an exam already imported.
    ImportAnswerKey {
        path: PathBuf,
        exam: DocumentId,
    },
    CancelBoard,
    DeleteBoardDocument(DocumentId),
    /// Reclassify material from another saved plan for the active plan.
    ReuseBoardDocument(DocumentId),
    /// Use an item for a topic, or set it aside (`topic: None`).
    SetBoardItem {
        item: BoardItem,
        relevance: Relevance,
        topic: Option<TopicId>,
    },

    /// Writes an already formatted progress report.
    ExportProgress {
        path: PathBuf,
        content: String,
    },
}

/// Top-level error, wrapping each layer's error.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Study(#[from] StudyError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Ai(#[from] AiError),
    #[error(transparent)]
    Audio(#[from] AudioError),
    #[error(transparent)]
    Pdf(#[from] PdfError),
    #[error(transparent)]
    Handout(#[from] HandoutError),
    #[error("cancelled")]
    Cancelled,
    #[error("could not write {path}: {source}")]
    Export {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("no edital imported")]
    NoEdital,
    #[error("no active study plan")]
    NoActivePlan,
    #[error("topic {0:?} not found")]
    TopicNotFound(TopicId),
    #[error("speech model {0:?} is not installed")]
    SpeechModelMissing(ModelId),
}

/// Something the UI should tell the user about.
#[derive(Debug)]
pub enum Notice {
    Error(EngineError),
    PlanCreated,
    SettingsSaved,
    ConnectionOk,
    ModelInstalled(ModelId),
    LessonSaved,
    ReviewSaved {
        next_due: Option<NaiveDate>,
    },
    QuizSaved {
        correct: u32,
        total: u32,
    },
    NothingHeard,
    Exported(PathBuf),
    HandoutSaved(PathBuf),
    ContentPrepared(PrepareSummary),
    BoardImported {
        kind: DocumentKind,
        used: usize,
        set_aside: usize,
    },
    AnswerKeyApplied {
        count: usize,
    },
    Milestone(Milestone),
}

/// State of the edital import on the Edital screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportState {
    Empty,
    Extracting { file: String },
    Ready(Edital),
    Failed,
}

/// Results from worker threads.
pub(crate) enum Event {
    EditalExtracted(Result<Edital, EngineError>),
    PlanGenerated {
        edital: Edital,
        prefs: PlanPrefs,
        result: Result<kg_ai::PlanDraft, AiError>,
    },
    ConnectionTested(Result<(), AiError>),
    Lesson(lesson::LessonEvent),
    Practice(practice::PracticeEvent),
    DownloadFinished(ModelId, Result<(), AudioError>),
    HandoutTopic {
        topic: TopicId,
        json: String,
    },
    HandoutFinished(Result<PathBuf, EngineError>),
    Prepare(prepare::PrepareEvent),
    BoardImported(Result<board::BoardImport, EngineError>),
    AnswerKeyRead {
        exam: DocumentId,
        result: Result<Vec<(u32, String)>, EngineError>,
    },
}

/// Called by workers after sending an event, so the UI wakes up.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// Owns all app state.
pub struct Engine {
    store: Store,
    paths: AppPaths,
    models: ModelStore,
    env_ai: AiConfig,
    settings: Settings,

    plans: Vec<PlanSummary>,
    active: Option<StudyPlan>,
    quiz_stats: QuizStats,
    lesson_count: u32,

    import: ImportState,
    generating_since: Option<Instant>,
    testing_connection: bool,

    lesson: Option<LessonSession>,
    quiz: Option<QuizSession>,
    review: Option<ReviewSession>,
    voice: lesson::VoiceRuntime,
    stt: Arc<Mutex<Option<(WhisperModel, WhisperStt)>>>,
    downloads: HashMap<ModelId, models::Download>,
    handout_job: Option<HandoutJob>,
    handouts_cached: usize,
    prepare_job: Option<PrepareJob>,
    /// Preparation asked for while the plan was still being generated.
    pending_prepare: Option<PrepareRequest>,
    board: board::BoardState,
    board_job: Option<BoardJob>,
    streak: u32,
    studied_today: bool,
    study_clock: study_time::StudyClock,

    notices: Vec<Notice>,
    events_tx: mpsc::Sender<Event>,
    events_rx: mpsc::Receiver<Event>,
    waker: Waker,
    next_session_id: u64,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("paths", &self.paths)
            .field("active", &self.active.as_ref().map(|p| p.id))
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// Opens the database in `paths` and loads settings. If the database
    /// file cannot be opened, runs on an in-memory one and reports why.
    pub fn new(paths: AppPaths, env_ai: AiConfig) -> Result<Self, EngineError> {
        let mut notices = Vec::new();
        let opened = paths
            .ensure()
            .map_err(|source| EngineError::Export {
                path: paths.data_dir.clone(),
                source,
            })
            .and_then(|()| Store::open(&paths.database()).map_err(EngineError::from));
        let store = match opened {
            Ok(store) => store,
            Err(err) => {
                tracing::error!(%err, "falling back to in-memory database");
                notices.push(Notice::Error(err));
                Store::open_in_memory()?
            }
        };
        Ok(Self::with_store(store, paths, env_ai, notices))
    }

    /// Engine on an in-memory database, for tests.
    pub fn in_memory(paths: AppPaths, env_ai: AiConfig) -> Result<Self, EngineError> {
        Ok(Self::with_store(
            Store::open_in_memory()?,
            paths,
            env_ai,
            Vec::new(),
        ))
    }

    fn with_store(
        store: Store,
        paths: AppPaths,
        env_ai: AiConfig,
        mut notices: Vec<Notice>,
    ) -> Self {
        let settings = Settings::load(&store, &env_ai).unwrap_or_else(|err| {
            notices.push(Notice::Error(err.into()));
            Settings {
                ai: env_ai.clone(),
                ..Settings::default()
            }
        });
        let (events_tx, events_rx) = mpsc::channel();
        let models = ModelStore::new(paths.models_dir());
        let mut engine = Self {
            store,
            paths,
            models,
            env_ai,
            settings,
            plans: Vec::new(),
            active: None,
            quiz_stats: QuizStats::default(),
            lesson_count: 0,
            import: ImportState::Empty,
            generating_since: None,
            testing_connection: false,
            lesson: None,
            quiz: None,
            review: None,
            voice: lesson::VoiceRuntime::default(),
            stt: Arc::new(Mutex::new(None)),
            downloads: HashMap::new(),
            handout_job: None,
            handouts_cached: 0,
            prepare_job: None,
            pending_prepare: None,
            board: board::BoardState::default(),
            board_job: None,
            streak: 0,
            studied_today: false,
            study_clock: study_time::StudyClock::new(Self::today(), Instant::now()),
            notices,
            events_tx,
            events_rx,
            waker: Arc::new(|| {}),
            next_session_id: 1,
        };
        engine.reload_plans();
        engine.refresh_activity();
        engine.load_study_time();
        engine
    }

    /// Sets the callback workers use to wake the UI.
    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = waker;
    }

    pub fn today() -> NaiveDate {
        chrono::Local::now().date_naive()
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// True when the API key comes from `.env`.
    pub fn api_key_from_env(&self) -> bool {
        !self.env_ai.api_key.is_empty() && self.settings.ai.api_key == self.env_ai.api_key
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    pub fn plans(&self) -> &[PlanSummary] {
        &self.plans
    }

    pub fn active_plan(&self) -> Option<&StudyPlan> {
        self.active.as_ref()
    }

    pub fn quiz_stats(&self) -> QuizStats {
        self.quiz_stats
    }

    pub fn lesson_count(&self) -> u32 {
        self.lesson_count
    }

    pub fn import_state(&self) -> &ImportState {
        &self.import
    }

    /// When plan generation started, while it runs.
    pub fn generating_since(&self) -> Option<Instant> {
        self.generating_since
    }

    pub fn is_testing_connection(&self) -> bool {
        self.testing_connection
    }

    pub fn lesson(&self) -> Option<&LessonSession> {
        self.lesson.as_ref()
    }

    pub fn quiz(&self) -> Option<&QuizSession> {
        self.quiz.as_ref()
    }

    pub fn review(&self) -> Option<&ReviewSession> {
        self.review.as_ref()
    }

    /// Notices produced since the last call.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    /// True while something is running in the background or audio is live,
    /// so the UI should keep repainting.
    pub fn is_busy(&self) -> bool {
        self.generating_since.is_some()
            || matches!(self.import, ImportState::Extracting { .. })
            || !self.downloads.is_empty()
            || self.handout_job.is_some()
            || self.prepare_job.is_some()
            || self.board_job.is_some()
            || self
                .lesson
                .as_ref()
                .is_some_and(|l| l.phase != LessonPhase::Idle)
            || self
                .quiz
                .as_ref()
                .is_some_and(|q| q.step == QuizStep::Loading)
            || self.review.as_ref().is_some_and(ReviewSession::is_loading)
    }

    pub fn handle(&mut self, command: Command) -> Result<(), EngineError> {
        match command {
            Command::ImportEdital(path) => self.import_edital(path),
            Command::ClearEdital => {
                self.import = ImportState::Empty;
                Ok(())
            }
            Command::GeneratePlan(prefs) => self.generate_plan(prefs),
            Command::SelectPlan(id) => self.select_plan(id),
            Command::DeletePlan(id) => self.delete_plan(id),
            Command::SetTopicStatus(id, status) => self.set_topic_status(id, status),

            Command::StartLesson(id) => self.start_lesson(id),
            Command::EndLesson => self.end_lesson(),
            Command::StartListening => self.start_listening(),
            Command::StopListening => {
                self.stop_listening();
                Ok(())
            }
            Command::SendMessage(text) => {
                self.send_student_message(text);
                Ok(())
            }
            Command::StopSpeaking => {
                self.stop_speaking();
                Ok(())
            }
            Command::PreviewVoice { settings, text } => self.preview_voice(settings, text),
            Command::StopVoicePreview => {
                self.stop_voice_preview();
                Ok(())
            }

            Command::StartQuiz(id) => self.start_quiz(id),
            Command::AnswerQuiz(option) => {
                self.answer_quiz(option);
                Ok(())
            }
            Command::NextQuestion => self.next_question(),
            Command::CloseQuiz => {
                self.quiz = None;
                Ok(())
            }

            Command::StartReview(id) => self.start_review(id),
            Command::SubmitReviewAnswer(answer) => {
                self.submit_review_answer(answer);
                Ok(())
            }
            Command::GradeReview(grade) => self.grade_review(grade),
            Command::CloseReview => {
                self.review = None;
                Ok(())
            }

            Command::SaveSettings(settings) => self.save_settings(settings),
            Command::TestConnection => {
                self.test_connection();
                Ok(())
            }
            Command::DownloadModel(id) => {
                self.download_model(id);
                Ok(())
            }
            Command::CancelDownload(id) => {
                self.cancel_download(id);
                Ok(())
            }
            Command::RemoveModel(id) => self.remove_model(id),

            Command::FinishOnboarding => self.finish_onboarding(),
            Command::ExportHandout(request) => self.export_handout(request),
            Command::ImportBoard { path, kind } => self.import_board(path, kind),
            Command::ReuseBoardDocument(id) => self.reuse_board_document(id),
            Command::ImportAnswerKey { path, exam } => self.import_answer_key(path, exam),
            Command::CancelBoard => {
                self.cancel_board();
                Ok(())
            }
            Command::DeleteBoardDocument(id) => self.delete_board_document(id),
            Command::SetBoardItem {
                item,
                relevance,
                topic,
            } => self.set_board_item(item, relevance, topic),
            Command::CancelHandout => {
                self.cancel_handout();
                Ok(())
            }
            Command::PrepareContent(request) => {
                if self.generating_since.is_some() {
                    self.pending_prepare = Some(request);
                    Ok(())
                } else {
                    self.prepare_content(request)
                }
            }
            Command::CancelPrepare => {
                self.pending_prepare = None;
                self.cancel_prepare();
                Ok(())
            }
            Command::ExportProgress { path, content } => {
                std::fs::write(&path, content).map_err(|source| EngineError::Export {
                    path: path.clone(),
                    source,
                })?;
                self.notices.push(Notice::Exported(path));
                Ok(())
            }
        }
    }

    /// Applies finished background work and advances audio state. Call once
    /// per frame.
    pub fn poll(&mut self) {
        self.tick_study_time();
        while let Ok(event) = self.events_rx.try_recv() {
            let result = match event {
                Event::EditalExtracted(result) => {
                    self.on_edital_extracted(result);
                    Ok(())
                }
                Event::PlanGenerated {
                    edital,
                    prefs,
                    result,
                } => self.on_plan_generated(edital, &prefs, result),
                Event::ConnectionTested(result) => {
                    self.testing_connection = false;
                    result
                        .map(|()| self.notices.push(Notice::ConnectionOk))
                        .map_err(EngineError::from)
                }
                Event::Lesson(event) => self.on_lesson_event(event),
                Event::Practice(event) => self.on_practice_event(event),
                Event::DownloadFinished(id, result) => self.on_download_finished(id, result),
                Event::HandoutTopic { topic, json } => self.on_handout_topic(topic, &json),
                Event::HandoutFinished(result) => self.on_handout_finished(result),
                Event::Prepare(event) => self.on_prepare_event(event),
                Event::BoardImported(result) => self.on_board_imported(result),
                Event::AnswerKeyRead { exam, result } => self.on_answer_key_read(exam, result),
            };
            if let Err(err) = result {
                tracing::warn!(%err, "background task failed");
                self.notices.push(Notice::Error(err));
            }
        }
        self.tick_lesson();
    }

    fn save_settings(&mut self, mut settings: Settings) -> Result<(), EngineError> {
        settings.normalize();
        let voice_changed = settings.voice != self.settings.voice
            || settings.tts_backend != self.settings.tts_backend
            || settings.cloud_speech != self.settings.cloud_speech
            || settings.speech_pitch != self.settings.speech_pitch
            || settings.speech_expressiveness != self.settings.speech_expressiveness
            || settings.speech_rhythm_variation != self.settings.speech_rhythm_variation
            || (settings.speech_rate - self.settings.speech_rate).abs() > f32::EPSILON;
        settings.save(&mut self.store, &self.env_ai)?;
        self.settings = settings;
        if voice_changed {
            self.stop_audio();
            self.voice.reset_tts();
        }
        self.notices.push(Notice::SettingsSaved);
        Ok(())
    }

    fn test_connection(&mut self) {
        if self.testing_connection {
            return;
        }
        self.testing_connection = true;
        let client = kg_ai::Client::new(self.settings.ai.clone());
        self.spawn(move || Event::ConnectionTested(client.ping()));
    }

    /// Runs `job` on a worker thread and delivers its event.
    pub(crate) fn spawn(&self, job: impl FnOnce() -> Event + Send + 'static) {
        let tx = self.events_tx.clone();
        let waker = Arc::clone(&self.waker);
        let spawned = std::thread::Builder::new()
            .name("kg-worker".into())
            .spawn(move || {
                let _ = tx.send(job());
                waker();
            });
        if let Err(err) = spawned {
            tracing::error!(%err, "could not spawn worker thread");
        }
    }

    pub(crate) fn sender(&self) -> (mpsc::Sender<Event>, Waker) {
        (self.events_tx.clone(), Arc::clone(&self.waker))
    }

    pub(crate) fn next_id(&mut self) -> u64 {
        self.next_session_id += 1;
        self.next_session_id
    }

    pub(crate) fn ai_client(&self) -> kg_ai::Client {
        kg_ai::Client::new(self.settings.ai.clone())
    }

    /// Prompt context for a topic of the active plan.
    pub(crate) fn topic_context(
        &self,
        id: TopicId,
    ) -> Result<(kg_ai::TopicContext, String), EngineError> {
        let plan = self.active.as_ref().ok_or(EngineError::NoActivePlan)?;
        let (subject, topic) = plan.find_topic(id).ok_or(EngineError::TopicNotFound(id))?;
        let exam = if plan.exam.is_empty() {
            plan.title.clone()
        } else {
            plan.exam.clone()
        };
        Ok((
            kg_ai::TopicContext {
                exam,
                subject: subject.name.clone(),
                topic: topic.title.clone(),
                reference: self.board_reference(id),
            },
            topic.title.clone(),
        ))
    }

    /// Changes a topic of the active plan and saves it.
    pub(crate) fn update_topic(
        &mut self,
        id: TopicId,
        change: impl FnOnce(&mut Topic),
    ) -> Result<(), EngineError> {
        let plan = self.active.as_mut().ok_or(EngineError::NoActivePlan)?;
        let topic = plan
            .find_topic_mut(id)
            .ok_or(EngineError::TopicNotFound(id))?;
        change(topic);
        self.store.update_topic(topic)?;
        self.refresh_stats();
        self.check_progress_milestone()
    }
}

#[cfg(test)]
mod tests;
