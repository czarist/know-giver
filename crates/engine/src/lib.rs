//! Engine: application state and orchestration between study, AI and audio.
//!
//! The UI reads state from [`Engine`] and sends it [`Command`]s. It never talks
//! to the AI, audio, PDF or storage layers directly. Slow work (PDF parsing,
//! model calls, speech) runs on worker threads; results come back as events
//! that [`Engine::poll`] applies on the UI thread.

mod board;
mod discursive;
mod documents;
mod handouts;
mod illustrations;
mod lesson;
mod models;
mod paths;
mod plans;
mod practice;
mod prepare;
mod progress;
mod reader;
mod settings;
mod setup;
mod stt;
mod study_time;
mod teacher;
mod update;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Instant;

use chrono::NaiveDate;

pub use board::{BoardItem, BoardJob};
pub use discursive::{
    AttemptProblems, EvolutionGroup, EvolutionRow, ImageProblem, NOTE_INTERRUPTED, NOTE_UNREADABLE,
    OpenAttempt, ProposalRequest, VisionCheck,
};
pub use handouts::{HandoutJob, HandoutRequest, HandoutScope};
pub use illustrations::{BoardImage, Illustration, illustration_key};
pub use kg_ai::{AiConfig, AiError, LessonLength, ModelSet, PlanPrefs, ReviewFeedback};
pub use kg_ai::{
    ModelChoice, PROVIDERS, Provider, ProviderKind, Recommendation, provider as ai_provider,
    provider_for_url,
};
pub use kg_audio::{
    AudioError, CloudSpeechConfig, EnglishVoice, KokoroVoice, ModelId, PiperVoice, SttEngine,
    TtsBackend, Voice, VoiceGender, WhisperModel,
};
pub use kg_audio::{EXPRESSIVENESS_RANGE, RHYTHM_VARIATION_RANGE};
pub use kg_handout::{
    CjkScript, Document as HandoutDocument, Exercise as HandoutExercise, HandoutError,
    Labels as HandoutLabels, Section as HandoutSection, TextBox, TopicHandout, cjk_font,
};
pub use kg_pdf::{Edital, EditalText, PdfError, Role};
pub use kg_study::discursive::{REWRITE_RATIO, change_ratio, marks as transcript_marks};
pub use kg_study::{
    Attempt, AttemptId, AttemptOrigin, AttemptStatus, Criterion, DifficultyReport, Evaluation,
    Evidence, Legibility, MAX_PAGES, Page, PageId, PageIssue, Problem, ProblemCategory, Proposal,
    ProposalId, RecurringIssue, Remark, Rubric, RubricError, RubricSource, Stage, TranscriptPage,
    TranscriptRecord, Unreadable, WritingKind, WritingRules,
};
pub use kg_study::{
    BoardDocument, BoardExcerpt, BoardQuestion, DocumentId, DocumentKind, Relevance,
};
pub use kg_study::{
    Difficulty, PlanId, PlanSummary, QuizQuestion, QuizStats, ReviewGrade, StoreError, StudyError,
    StudyPlan, Subject, SubjectId, Topic, TopicId, TopicStatus,
};
pub use kg_study::{Highlight, HighlightColor, HighlightId, Reading, ReadingId};
pub use kg_study::{LessonRecap, Mistake, MistakeId};
pub use lesson::{LessonPhase, LessonSession, LessonTurn, Speaker};
pub use models::ModelStatus;
pub use paths::AppPaths;
pub use practice::{QuizSession, QuizStep, REVIEW_ROUNDS, ReviewSession, ReviewStep};
pub use prepare::{PrepareJob, PrepareRequest, PrepareSummary, slug};
pub use progress::Milestone;
pub use reader::{ContentsEntry, ReaderState};
pub use settings::{
    DEFAULT_TURN_PAUSE, Region, SETUP_VERSION, Settings, TERMS_VERSION, TURN_PAUSE_RANGE,
    TermsAcceptance,
};
pub use setup::ModelList;
pub use study_time::StudyPresence;
pub use teacher::Teacher;
pub use update::{REPOSITORY, Release, UpdateStatus};

use kg_audio::ModelStore;
use kg_study::Store;

/// Requests from the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    ImportEdital(PathBuf),
    ClearEdital,
    /// Reads the notice again (a new or amended PDF) for a saved plan. Only
    /// the notice text changes: topics, progress, handouts and written
    /// answers stay.
    RevalidateEdital {
        plan: PlanId,
        path: PathBuf,
    },
    GeneratePlan(PlanPrefs),
    SelectPlan(PlanId),
    DeletePlan(PlanId),
    SetTopicStatus(TopicId, TopicStatus),

    /// Starts a lesson from the beginning.
    StartLesson(TopicId),
    /// Continues the unfinished lesson on a topic (or starts one).
    ResumeLesson(TopicId),
    EndLesson,
    /// Looks back at the last lesson taken on a topic.
    OpenRecap(TopicId),
    CloseRecap,
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
    /// A quiz with only the questions of this topic answered wrong before.
    RetryMistakes(TopicId),
    /// Takes a question out of the mistakes notebook.
    DeleteMistake(MistakeId),
    AnswerQuiz(usize),
    NextQuestion,
    CloseQuiz,

    StartReview(TopicId),
    SubmitReviewAnswer(String),
    /// After a graded answer, the next question of the review.
    NextReviewQuestion,
    /// Answer the review question out loud.
    StartReviewDictation,
    /// Stop answering out loud; the text typed so far goes first.
    StopReviewDictation(String),
    /// Read a text aloud in the tutor's voice.
    ReadAloud(String),
    GradeReview(ReviewGrade),
    CloseReview,

    SaveSettings(Settings),
    TestConnection,
    /// Tests settings not saved yet (first-run setup).
    TestAiConfig(Box<AiConfig>),
    /// Asks the service which models a key can use.
    ListModels(Box<AiConfig>),
    /// Saves the first-run setup and the agreement to the terms.
    CompleteSetup(Box<Settings>),
    /// Ends (or skips) the guided tour of the screens.
    FinishTour,
    /// Asks GitHub whether a newer version exists (when checks are on).
    CheckForUpdate,
    /// Downloads and installs the version on offer, then closes the app.
    InstallUpdate,
    /// Hides the update offer until the app opens again.
    DismissUpdate,
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

    /// Upload a past exam or board material for the active plan. An exam
    /// can come with its answer key in a separate file, read right after.
    ImportBoard {
        path: PathBuf,
        kind: DocumentKind,
        answer_key: Option<PathBuf>,
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

    /// Opens a kept handout in the reader.
    OpenReading(ReadingId),
    CloseReader,
    DeleteReading(ReadingId),
    /// The page the student is looking at in the reader.
    ReaderGoTo(usize),
    /// Draws a reader page at `scale` (pixels per point × 100).
    RenderPage {
        page: usize,
        scale: u32,
    },
    /// Highlights text boxes `first..=last` of a page.
    AddHighlight {
        page: usize,
        first: usize,
        last: usize,
        color: HighlightColor,
    },
    UpdateHighlight {
        id: HighlightId,
        color: HighlightColor,
        note: String,
    },
    DeleteHighlight(HighlightId),
    /// Saves the open handout as a PDF with its highlights.
    ExportReading(PathBuf),

    /// Asks for a written-answer proposal on the active plan.
    GenerateProposal(ProposalRequest),
    DeleteProposal(ProposalId),
    /// A new attempt on a proposal, opened to add its pages.
    StartAttempt(ProposalId),
    OpenAttempt(AttemptId),
    CloseAttempt,
    /// Photos (JPEG or PNG) added after the open attempt's pages.
    AddPages(Vec<PathBuf>),
    ReplacePage {
        page: PageId,
        path: PathBuf,
    },
    /// Turns a page a quarter clockwise.
    RotatePage(PageId),
    MovePage {
        page: PageId,
        up: bool,
    },
    RemovePage(PageId),
    /// Sends the open attempt's pages to the vision model to be read.
    SubmitPages,
    /// Confirms the checked transcript (one text per page) and grades it.
    ConfirmTranscript(Vec<String>),
    /// Saves a rewritten text as a new attempt and grades that one.
    SaveRewrite(Vec<String>),
    /// Runs the failed step of the open attempt again.
    RetryAttempt,
    /// Back to checking the transcript of a pending attempt.
    ReopenTranscript,
    DeleteAttempt(AttemptId),
    /// Looks for difficulties repeated across graded attempts.
    AnalyzeWriting,
    /// Checks that the vision model really reads images.
    CheckVision,

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
    #[error("page photo refused: {0:?}")]
    Image(ImageProblem),
    #[error("the exam notice describes no written test of this kind")]
    NoWrittenTest,
    #[error("too many pages for one attempt")]
    TooManyPages,
    #[error("no pages to send")]
    NoPages,
    #[error("a page must be photographed again")]
    PageNeedsRetake,
    #[error("the attempt cannot be changed in its current state")]
    AttemptLocked,
    #[error("the corrections change the answer itself")]
    LooksLikeRewrite,
    #[error("fewer than two graded attempts")]
    NotEnoughAttempts,
    #[error(transparent)]
    Rubric(#[from] RubricError),
    #[error("the first-run setup is missing a choice")]
    SetupIncomplete,
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
    ProposalCreated,
    /// A plan's notice was read again; whether it has a written test.
    EditalRevalidated {
        written_test: bool,
    },
    /// A written answer was graded (estimated score).
    WritingEvaluated {
        total: f64,
        max: f64,
    },
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
    /// A whiteboard picture search finished.
    Illustration {
        query: String,
        image: Option<illustrations::BoardImage>,
    },
    /// Roles the model read in an edital that does not number them.
    RolesFound {
        file: String,
        result: Result<Vec<String>, AiError>,
    },
    PlanGenerated {
        edital: Edital,
        prefs: PlanPrefs,
        result: Result<kg_ai::PlanDraft, AiError>,
    },
    ConnectionTested(Box<AiConfig>, Result<(), AiError>),
    ModelsListed(setup::ModelList),
    Lesson(lesson::LessonEvent),
    Practice(practice::PracticeEvent),
    DownloadFinished(ModelId, Result<(), AudioError>),
    HandoutTopic {
        topic: TopicId,
        json: String,
    },
    HandoutFinished(Result<reader::SavedHandout, EngineError>),
    Reader(reader::ReaderEvent),
    Prepare(prepare::PrepareEvent),
    BoardImported(Result<board::BoardImport, EngineError>),
    AnswerKeyRead {
        exam: DocumentId,
        result: Result<Vec<(u32, String)>, EngineError>,
    },
    Discursive(discursive::DiscursiveEvent),
    EditalRevalidated {
        plan: PlanId,
        result: Result<Edital, EngineError>,
    },
    UpdateChecked(Result<Option<update::Release>, String>),
    UpdateInstalled(Result<(), String>),
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
    /// Open mistakes of the active plan, latest first.
    mistakes: Vec<Mistake>,
    /// Topics with an unfinished lesson to continue.
    lesson_drafts: Vec<TopicId>,
    /// A taken lesson being looked back at.
    recap: Option<(TopicId, Option<LessonRecap>)>,

    import: ImportState,
    /// Roles (cargos) of the imported edital; empty when none were found.
    roles: Vec<Role>,
    finding_roles: bool,
    generating_since: Option<Instant>,
    testing_connection: bool,
    /// Last connection test: the settings tested and whether it worked.
    last_test: Option<(AiConfig, bool)>,
    listing_models: bool,
    model_list: Option<setup::ModelList>,

    lesson: Option<LessonSession>,
    quiz: Option<QuizSession>,
    review: Option<ReviewSession>,
    voice: lesson::VoiceRuntime,
    stt: stt::Recognizers,
    illustrations: illustrations::Illustrations,
    downloads: HashMap<ModelId, models::Download>,
    handout_job: Option<HandoutJob>,
    handouts_cached: usize,
    readings: Vec<Reading>,
    reader: Option<ReaderState>,
    prepare_job: Option<PrepareJob>,
    /// Preparation asked for while the plan was still being generated.
    pending_prepare: Option<PrepareRequest>,
    board: board::BoardState,
    board_job: Option<BoardJob>,
    discursive: discursive::DiscursiveState,
    /// Plan whose notice is being read again.
    revalidating: Option<PlanId>,
    streak: u32,
    studied_today: bool,
    study_clock: study_time::StudyClock,
    updates: update::Updates,

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
            mistakes: Vec::new(),
            lesson_drafts: Vec::new(),
            recap: None,
            lesson_count: 0,
            import: ImportState::Empty,
            roles: Vec::new(),
            finding_roles: false,
            generating_since: None,
            testing_connection: false,
            last_test: None,
            listing_models: false,
            model_list: None,
            lesson: None,
            quiz: None,
            review: None,
            voice: lesson::VoiceRuntime::default(),
            stt: stt::Recognizers::default(),
            illustrations: illustrations::Illustrations::default(),
            downloads: HashMap::new(),
            handout_job: None,
            readings: Vec::new(),
            reader: None,
            handouts_cached: 0,
            prepare_job: None,
            pending_prepare: None,
            board: board::BoardState::default(),
            board_job: None,
            discursive: discursive::DiscursiveState::default(),
            revalidating: None,
            streak: 0,
            studied_today: false,
            study_clock: study_time::StudyClock::new(Self::today(), Instant::now()),
            updates: update::Updates::default(),
            notices,
            events_tx,
            events_rx,
            waker: Arc::new(|| {}),
            next_session_id: 1,
        };
        engine.recover_writing();
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

    /// The topic has an unfinished lesson to continue.
    pub fn has_lesson_draft(&self, topic: TopicId) -> bool {
        self.lesson_drafts.contains(&topic)
    }

    /// The lesson being looked back at: its topic and what it covered
    /// (`None` when no lesson on it was kept).
    pub fn recap(&self) -> Option<(TopicId, Option<&LessonRecap>)> {
        self.recap.as_ref().map(|(id, recap)| (*id, recap.as_ref()))
    }

    /// Quiz questions answered wrong and not yet right, latest first.
    pub fn mistakes(&self) -> &[Mistake] {
        &self.mistakes
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

    /// Roles (cargos) of the imported edital. With two or more, the student
    /// must pick the one to study for.
    pub fn edital_roles(&self) -> &[Role] {
        &self.roles
    }

    /// True while the model looks for roles in the imported edital.
    pub fn finding_roles(&self) -> bool {
        self.finding_roles
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
            || self.writing_busy()
            || self.revalidating.is_some()
    }

    /// The plan whose notice is being read again, while it is.
    pub fn revalidating(&self) -> Option<PlanId> {
        self.revalidating
    }

    pub fn handle(&mut self, command: Command) -> Result<(), EngineError> {
        match command {
            Command::ImportEdital(path) => self.import_edital(path),
            Command::ClearEdital => {
                self.import = ImportState::Empty;
                self.roles.clear();
                self.finding_roles = false;
                Ok(())
            }
            Command::RevalidateEdital { plan, path } => self.revalidate_edital(plan, path),
            Command::GeneratePlan(prefs) => self.generate_plan(prefs),
            Command::SelectPlan(id) => self.select_plan(id),
            Command::DeletePlan(id) => self.delete_plan(id),
            Command::SetTopicStatus(id, status) => self.set_topic_status(id, status),

            Command::OpenReading(id) => self.open_reading(id),
            Command::CloseReader => {
                self.close_reader();
                Ok(())
            }
            Command::DeleteReading(id) => self.delete_reading(id),
            Command::ReaderGoTo(page) => {
                self.reader_go_to(page);
                Ok(())
            }
            Command::RenderPage { page, scale } => {
                self.render_page(page, scale);
                Ok(())
            }
            Command::AddHighlight {
                page,
                first,
                last,
                color,
            } => self.add_highlight(page, first, last, color),
            Command::UpdateHighlight { id, color, note } => self.update_highlight(id, color, note),
            Command::DeleteHighlight(id) => self.delete_highlight(id),
            Command::ExportReading(path) => {
                self.export_reading(path);
                Ok(())
            }

            Command::StartLesson(id) => {
                self.recap = None;
                self.start_lesson(id)
            }
            Command::EndLesson => self.end_lesson(),
            Command::ResumeLesson(id) => {
                self.recap = None;
                self.resume_lesson(id)
            }
            Command::OpenRecap(id) => {
                self.recap = Some((id, self.store.last_lesson(id)?));
                Ok(())
            }
            Command::CloseRecap => {
                self.recap = None;
                Ok(())
            }
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
            Command::RetryMistakes(id) => self.retry_mistakes(id),
            Command::DeleteMistake(id) => {
                self.store.delete_mistake(id)?;
                self.refresh_mistakes();
                Ok(())
            }
            Command::AnswerQuiz(option) => self.answer_quiz(option),
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
            Command::NextReviewQuestion => {
                self.next_review_question();
                Ok(())
            }
            Command::StartReviewDictation => self.start_review_dictation(),
            Command::StopReviewDictation(typed) => {
                self.stop_review_dictation(typed);
                Ok(())
            }
            Command::ReadAloud(text) => self.say(&text),
            Command::CloseReview => {
                self.stop_speaking();
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
            Command::TestAiConfig(config) => {
                self.test_config(*config);
                Ok(())
            }
            Command::ListModels(config) => {
                self.list_models(*config);
                Ok(())
            }
            Command::CompleteSetup(settings) => self.complete_setup(*settings),
            Command::FinishTour => self.finish_tour(),
            Command::CheckForUpdate => {
                self.check_for_update();
                Ok(())
            }
            Command::InstallUpdate => {
                self.install_update();
                Ok(())
            }
            Command::DismissUpdate => {
                self.dismiss_update();
                Ok(())
            }
            Command::ExportHandout(request) => self.export_handout(request),
            Command::ImportBoard {
                path,
                kind,
                answer_key,
            } => self.import_board(path, kind, answer_key),
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
            Command::GenerateProposal(request) => self.generate_proposal(request),
            Command::DeleteProposal(id) => self.delete_proposal(id),
            Command::StartAttempt(id) => self.start_attempt(id),
            Command::OpenAttempt(id) => self.open_writing_attempt(id),
            Command::CloseAttempt => {
                self.close_attempt();
                Ok(())
            }
            Command::AddPages(paths) => self.add_pages(paths),
            Command::ReplacePage { page, path } => self.replace_page(page, path),
            Command::RotatePage(page) => self.rotate_page(page),
            Command::MovePage { page, up } => self.move_page(page, up),
            Command::RemovePage(page) => self.remove_page(page),
            Command::SubmitPages => self.submit_pages(),
            Command::ConfirmTranscript(pages) => self.confirm_transcript(pages),
            Command::SaveRewrite(pages) => self.save_rewrite(pages),
            Command::RetryAttempt => self.retry_attempt(),
            Command::ReopenTranscript => self.reopen_transcript(),
            Command::DeleteAttempt(id) => self.delete_attempt(id),
            Command::AnalyzeWriting => self.analyze_writing(),
            Command::CheckVision => self.check_vision(),
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
                Event::Illustration { query, image } => {
                    self.on_illustration(query, image);
                    Ok(())
                }
                Event::RolesFound { file, result } => {
                    self.on_roles_found(&file, result);
                    Ok(())
                }
                Event::PlanGenerated {
                    edital,
                    prefs,
                    result,
                } => self.on_plan_generated(edital, &prefs, result),
                Event::ConnectionTested(config, result) => {
                    self.testing_connection = false;
                    self.last_test = Some((*config, result.is_ok()));
                    result
                        .map(|()| self.notices.push(Notice::ConnectionOk))
                        .map_err(EngineError::from)
                }
                Event::ModelsListed(list) => {
                    self.on_models_listed(list);
                    Ok(())
                }
                Event::Lesson(event) => self.on_lesson_event(event),
                Event::Practice(event) => self.on_practice_event(event),
                Event::DownloadFinished(id, result) => self.on_download_finished(id, result),
                Event::HandoutTopic { topic, json } => self.on_handout_topic(topic, &json),
                Event::HandoutFinished(result) => self.on_handout_finished(result),
                Event::Reader(event) => self.on_reader_event(event),
                Event::Prepare(event) => self.on_prepare_event(event),
                Event::BoardImported(result) => self.on_board_imported(result),
                Event::AnswerKeyRead { exam, result } => self.on_answer_key_read(exam, result),
                Event::UpdateChecked(result) => {
                    self.on_update_checked(result);
                    Ok(())
                }
                Event::UpdateInstalled(result) => {
                    self.on_update_installed(result);
                    Ok(())
                }
                Event::Discursive(event) => self.on_discursive_event(event),
                Event::EditalRevalidated { plan, result } => {
                    self.on_edital_revalidated(plan, result)
                }
            };
            if let Err(err) = result {
                tracing::warn!(%err, "background task failed");
                self.notices.push(Notice::Error(err));
            }
        }
        self.tick_lesson();
        self.tick_reader();
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
        self.test_config(self.settings.ai.clone());
    }

    fn test_config(&mut self, config: AiConfig) {
        if self.testing_connection {
            return;
        }
        self.testing_connection = true;
        let client = kg_ai::Client::new(config.clone());
        self.spawn(move || Event::ConnectionTested(Box::new(config), client.ping()));
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

    /// Prompts follow the app's language.
    pub(crate) fn prompt_language(&self) -> kg_ai::PromptLanguage {
        kg_ai::PromptLanguage::from_locale(&self.settings.locale)
    }

    pub(crate) fn ai_client(&self) -> kg_ai::Client {
        kg_ai::Client::new(self.settings.ai.clone()).with_language(self.prompt_language())
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
                student: self.settings.student_name.trim().to_owned(),
                teacher_name: self
                    .settings
                    .teacher()
                    .name_in(self.prompt_language())
                    .to_owned(),
                teacher_female: self.settings.teacher().gender() == VoiceGender::Female,
                outline: Vec::new(),
                step: 0,
                step_notes: Vec::new(),
                quiz_topics: Vec::new(),
                length: self.settings.lesson_length,
                english_voice: self.settings.english_voice.is_some()
                    && self.models.is_installed(kg_audio::ModelId::KokoroModel)
                    && self.models.is_installed(kg_audio::ModelId::KokoroRuntime),
                mistakes: self
                    .store
                    .topic_mistakes(id)
                    .unwrap_or_default()
                    .iter()
                    .map(|m| {
                        m.summary_in(
                            self.prompt_language().base() == kg_ai::PromptLanguage::English,
                        )
                    })
                    .collect(),
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
