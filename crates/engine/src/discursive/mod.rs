//! Written answers ("Discursivas"): proposals from the plan, photographed
//! handwritten pages, a transcript the student checks, a formative grade
//! against a rubric and the history across attempts.
//!
//! Every background result names the attempt and the run it belongs to;
//! the attempt's saved state decides whether it still applies. Nothing a
//! student wrote, no image and no key is logged, only ids and counts.

mod images;
mod validate;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::NaiveDateTime;
use kg_ai::{AiError, EvaluationInput, IssueInput, ProposalDraft, ProposalInput};
use kg_study::discursive::{REWRITE_RATIO, change_ratio, edits, writing_rules};
use kg_study::{
    Attempt, AttemptId, AttemptOrigin, AttemptStatus, ConfirmedTranscript, DifficultyReport,
    Evaluation, Legibility, MAX_PAGES, Page, PageId, PageIssue, PlanId, ProblemCategory, Proposal,
    ProposalId, RecurringIssue, Rubric, Stage, TopicId, Transcript, TranscriptPage,
    TranscriptRecord, Unreadable, WritingRules,
};

pub use images::ImageProblem;
pub use validate::ProposalRequest;

use crate::{Engine, EngineError, Event, Notice};

/// Saved as the note of an attempt that was processing when the app closed.
pub const NOTE_INTERRUPTED: &str = "interrupted";
/// Saved as the note of an attempt not graded because too little was read.
pub const NOTE_UNREADABLE: &str = "unreadable";
/// Evaluations sent to look for recurring difficulties.
const ISSUE_ATTEMPTS: usize = 8;
/// Topics offered to the model when it suggests what to write about.
const SUGGESTED_TOPICS: usize = 12;
/// Characters of board material sent with a proposal request.
const REFERENCE_CHARS: usize = 4_000;

/// The attempt on screen, with everything about it.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenAttempt {
    pub attempt: Attempt,
    pub proposal: Proposal,
    pub pages: Vec<Page>,
    pub transcript: Option<TranscriptRecord>,
    pub evaluation: Option<Evaluation>,
}

/// Whether the configured vision model was seen reading an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisionCheck {
    #[default]
    Unknown,
    Checking,
    Works,
    /// It answered, but not with what the test image shows.
    NotReading,
}

/// Scores of attempts graded with the same rubric, comparable.
#[derive(Debug, Clone, PartialEq)]
pub struct EvolutionGroup {
    pub rubric: Rubric,
    pub rows: Vec<EvolutionRow>,
    /// More than one model or prompt version graded these attempts, so a
    /// change in scores is not only the student's.
    pub mixed_graders: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvolutionRow {
    pub attempt: AttemptId,
    pub number: u32,
    pub theme: String,
    pub evaluated_at: NaiveDateTime,
    /// Each criterion's score as a share of its points, in rubric order.
    pub shares: Vec<f64>,
    pub total: f64,
    pub max_total: f64,
}

/// Problems of one graded attempt, by where they come from: reading the
/// photo, or the student's writing and content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttemptProblems {
    /// Uncertain or illegible passages and photo issues.
    pub reading: usize,
    pub writing: usize,
    pub content: usize,
    pub structure: usize,
}

#[derive(Debug, Default)]
pub(crate) struct DiscursiveState {
    plan: Option<PlanId>,
    rules: WritingRules,
    proposals: Vec<Proposal>,
    attempts: Vec<Attempt>,
    evaluations: Vec<(AttemptId, Evaluation)>,
    problems: HashMap<AttemptId, AttemptProblems>,
    report: Option<DifficultyReport>,
    open: Option<OpenAttempt>,
    /// Prepared JPEG of each page on screen, with a version for the UI.
    images: HashMap<PageId, (u64, Arc<[u8]>)>,
    image_version: u64,
    generating: bool,
    analyzing: bool,
    /// Photos being read and prepared.
    adding: usize,
    vision: VisionCheck,
}

pub(crate) enum DiscursiveEvent {
    Proposal {
        plan: PlanId,
        request: ProposalRequest,
        candidates: Vec<TopicId>,
        edital: String,
        model: String,
        result: Box<Result<ProposalDraft, AiError>>,
    },
    PageStored {
        attempt: AttemptId,
        replace: Option<PageId>,
        result: Result<images::PreparedPage, EngineError>,
    },
    PageRotated {
        page: PageId,
        result: Result<images::PreparedPage, EngineError>,
    },
    PageImage {
        page: PageId,
        bytes: Arc<[u8]>,
    },
    Transcribed {
        attempt: AttemptId,
        run: u32,
        model: String,
        result: Result<Vec<(TranscriptPage, Vec<PageIssue>)>, EngineError>,
    },
    Evaluated {
        attempt: AttemptId,
        run: u32,
        result: Result<validate::Outcome, EngineError>,
    },
    Issues {
        plan: PlanId,
        attempts: Vec<(AttemptId, Vec<String>)>,
        model: String,
        result: Result<Vec<kg_ai::IssueDraft>, AiError>,
    },
    VisionChecked(Result<bool, AiError>),
}

fn now() -> NaiveDateTime {
    chrono::Local::now().naive_local()
}

impl Engine {
    /// What the active plan's notice says about written tests. The area is
    /// offered only when it describes one.
    pub fn writing_rules(&self) -> &WritingRules {
        &self.discursive.rules
    }

    /// Proposals of the active plan, newest first.
    pub fn proposals(&self) -> &[Proposal] {
        &self.discursive.proposals
    }

    /// Attempts of the active plan, oldest first.
    pub fn writing_attempts(&self) -> &[Attempt] {
        &self.discursive.attempts
    }

    pub fn writing_evaluations(&self) -> &[(AttemptId, Evaluation)] {
        &self.discursive.evaluations
    }

    pub fn attempt_problems(&self, attempt: AttemptId) -> Option<AttemptProblems> {
        self.discursive.problems.get(&attempt).copied()
    }

    pub fn difficulty_report(&self) -> Option<&DifficultyReport> {
        self.discursive.report.as_ref()
    }

    pub fn open_attempt(&self) -> Option<&OpenAttempt> {
        self.discursive.open.as_ref()
    }

    /// A page's prepared photo and its version (changes when it is turned).
    pub fn page_image(&self, page: PageId) -> Option<(u64, Arc<[u8]>)> {
        self.discursive.images.get(&page).cloned()
    }

    pub fn is_generating_proposal(&self) -> bool {
        self.discursive.generating
    }

    pub fn is_analyzing_writing(&self) -> bool {
        self.discursive.analyzing
    }

    /// Photos still being read and prepared.
    pub fn pages_in_progress(&self) -> usize {
        self.discursive.adding
    }

    pub fn vision_check(&self) -> VisionCheck {
        self.discursive.vision
    }

    /// The vision model and the server photos go to, when one is set.
    pub fn vision_target(&self) -> Option<(String, String)> {
        let model = self.settings.ai.models.vision.trim();
        (!model.is_empty()).then(|| (model.to_owned(), host(&self.settings.ai.base_url)))
    }

    /// What a full answer covers. Hidden until an attempt on the proposal
    /// was graded.
    pub fn expected_points<'a>(&self, proposal: &'a Proposal) -> Option<&'a [String]> {
        self.discursive
            .attempts
            .iter()
            .any(|a| a.proposal_id == proposal.id && a.status == AttemptStatus::Evaluated)
            .then_some(proposal.expected_points.as_slice())
    }

    /// Scores grouped by rubric: only attempts graded on the same criteria
    /// and points are compared.
    pub fn writing_evolution(&self) -> Vec<EvolutionGroup> {
        // (rubric key, group, (model, prompt version) of each grader)
        type Graders = Vec<(String, String)>;
        let mut groups: Vec<(String, EvolutionGroup, Graders)> = Vec::new();
        for (id, eval) in &self.discursive.evaluations {
            let Some(attempt) = self.discursive.attempts.iter().find(|a| a.id == *id) else {
                continue;
            };
            let theme = self
                .discursive
                .proposals
                .iter()
                .find(|p| p.id == attempt.proposal_id)
                .map(|p| p.theme.clone())
                .unwrap_or_default();
            let row = EvolutionRow {
                attempt: *id,
                number: attempt.number,
                theme,
                evaluated_at: eval.evaluated_at,
                shares: eval
                    .rubric
                    .criteria
                    .iter()
                    .map(|c| eval.share(&c.key).unwrap_or(0.0))
                    .collect(),
                total: eval.total(),
                max_total: eval.max_total(),
            };
            let key = eval.rubric.key();
            let grader = (eval.model.clone(), eval.prompt_version.clone());
            match groups.iter_mut().find(|(k, ..)| *k == key) {
                Some((_, group, graders)) => {
                    group.rows.push(row);
                    if !graders.contains(&grader) {
                        graders.push(grader);
                    }
                }
                None => groups.push((
                    key,
                    EvolutionGroup {
                        rubric: eval.rubric.clone(),
                        rows: vec![row],
                        mixed_graders: false,
                    },
                    vec![grader],
                )),
            }
        }
        groups
            .into_iter()
            .map(|(_, mut group, graders)| {
                group.mixed_graders = graders.len() > 1;
                group
            })
            .collect()
    }

    /// Reloads the active plan's written answers.
    pub(crate) fn refresh_discursive(&mut self) {
        let Some(plan) = self.active.as_ref().map(|p| p.id) else {
            let vision = self.discursive.vision;
            self.discursive = DiscursiveState {
                vision,
                ..DiscursiveState::default()
            };
            return;
        };
        let state = &mut self.discursive;
        if state.plan != Some(plan) {
            state.plan = Some(plan);
            state.rules = writing_rules(&self.store.edital_text(plan).unwrap_or_default());
            state.open = None;
            state.images.clear();
            state.report = None;
        }
        let loaded = (|| -> Result<(), EngineError> {
            state.proposals = self.store.proposals(plan)?;
            state.attempts = self.store.attempts(plan)?;
            state.evaluations = self.store.evaluations(plan)?;
            state.report = self.store.difficulty_report(plan)?;
            state.problems.clear();
            for (id, eval) in &state.evaluations {
                let mut problems = AttemptProblems::default();
                for p in &eval.problems {
                    match p.category {
                        ProblemCategory::Writing => problems.writing += 1,
                        ProblemCategory::Content => problems.content += 1,
                        ProblemCategory::Structure => problems.structure += 1,
                    }
                }
                if let Some(t) = self.store.transcript(*id)? {
                    problems.reading += t
                        .original
                        .pages
                        .iter()
                        .map(|p| p.uncertain.len() + usize::from(p.retake.is_some()))
                        .sum::<usize>();
                }
                problems.reading += self
                    .store
                    .pages(*id)?
                    .iter()
                    .map(|p| p.issues.len())
                    .sum::<usize>();
                state.problems.insert(*id, problems);
            }
            if let Some(open) = &state.open {
                state.open = load_open(&self.store, open.attempt.id)?;
            }
            Ok(())
        })();
        if let Err(err) = loaded {
            self.notices.push(Notice::Error(err));
        }
    }

    /// Reads the notice's written tests again on the next refresh.
    pub(crate) fn forget_writing_rules(&mut self) {
        self.discursive.plan = None;
    }

    /// Marks attempts left processing by a closed app as failed.
    pub(crate) fn recover_writing(&mut self) {
        match self
            .store
            .fail_interrupted_attempts(NOTE_INTERRUPTED, now())
        {
            Ok(0) => {}
            Ok(count) => tracing::info!(count, "interrupted written attempts marked failed"),
            Err(err) => self.notices.push(Notice::Error(err.into())),
        }
    }

    /// Removes the photo folders of a plan's attempts before it is deleted.
    pub(crate) fn remove_plan_writing(&mut self, plan: PlanId) {
        for attempt in self.store.plan_attempt_ids(plan).unwrap_or_default() {
            self.remove_attempt_files(attempt);
        }
    }

    pub(crate) fn close_attempt(&mut self) {
        self.discursive.open = None;
        self.discursive.images.clear();
    }

    pub(crate) fn generate_proposal(
        &mut self,
        request: ProposalRequest,
    ) -> Result<(), EngineError> {
        let plan = self.active.as_ref().ok_or(EngineError::NoActivePlan)?;
        if !self.discursive.rules.kinds.contains(&request.kind) {
            return Err(EngineError::NoWrittenTest);
        }
        if self.discursive.generating {
            return Ok(());
        }
        if let Some(text) = &request.manual_rubric {
            kg_study::discursive::parse_manual_rubric(text)?;
        }
        let candidates: Vec<TopicId> = if request.topics.is_empty() {
            self.suggested_topics()
        } else {
            request
                .topics
                .iter()
                .copied()
                .filter(|t| plan.find_topic(*t).is_some())
                .collect()
        };
        if candidates.is_empty() {
            return Err(EngineError::NoActivePlan);
        }
        let titles: Vec<String> = candidates
            .iter()
            .filter_map(|t| plan.find_topic(*t))
            .map(|(s, t)| format!("{} — {}", s.name, t.title))
            .collect();
        let mut history = Vec::new();
        for id in &candidates {
            if let Some((_, topic)) = plan.find_topic(*id) {
                let mistakes = self.mistakes.iter().filter(|m| m.topic_id == *id).count();
                let lesson = if topic.lesson_taken() {
                    "aula feita"
                } else {
                    "sem aula"
                };
                history.push(format!(
                    "{}: {lesson}, {mistakes} erro(s) no quiz",
                    topic.title
                ));
            }
        }
        if let Some((_, last)) = self.discursive.evaluations.last() {
            for priority in &last.priorities {
                history.push(format!("prioridade da última avaliação: {priority}"));
            }
        }
        let mut reference = String::new();
        for id in &candidates {
            let block = self.board_reference(*id);
            if reference.len() + block.len() <= REFERENCE_CHARS {
                reference.push_str(&block);
            }
        }
        let exam = if plan.exam.is_empty() {
            plan.title.clone()
        } else {
            plan.exam.clone()
        };
        let input = ProposalInput {
            exam,
            kind: request.kind.prompt_name().to_owned(),
            difficulty: match request.difficulty {
                kg_study::Difficulty::Easy => "fácil",
                kg_study::Difficulty::Medium => "média",
                kg_study::Difficulty::Hard => "difícil",
            }
            .to_owned(),
            topics: titles,
            history,
            reference,
            edital: self.discursive.rules.excerpt.clone(),
            manual_rubric: request.manual_rubric.clone().unwrap_or_default(),
        };
        let plan_id = plan.id;
        let client = self.ai_client();
        let model = client.config().models.quiz.clone();
        self.discursive.generating = true;
        tracing::info!(
            topics = candidates.len(),
            kind = request.kind.as_str(),
            "writing proposal requested"
        );
        self.spawn(move || {
            let result = Box::new(kg_ai::generate_proposal(&client, &input));
            Event::Discursive(DiscursiveEvent::Proposal {
                plan: plan_id,
                request,
                edital: input.edital,
                candidates,
                model,
                result,
            })
        });
        Ok(())
    }

    /// Topics to suggest a proposal on: those the last grades asked to
    /// review, those with quiz mistakes, then studied ones, then this
    /// week's.
    fn suggested_topics(&self) -> Vec<TopicId> {
        let Some(plan) = self.active.as_ref() else {
            return Vec::new();
        };
        let mut out: Vec<TopicId> = Vec::new();
        let mut push = |id: TopicId| {
            if !out.contains(&id) && plan.find_topic(id).is_some() {
                out.push(id);
            }
        };
        for (_, eval) in self.discursive.evaluations.iter().rev() {
            eval.review_topics.iter().copied().for_each(&mut push);
        }
        self.mistakes.iter().map(|m| m.topic_id).for_each(&mut push);
        plan.topics()
            .filter(|(_, t)| t.lesson_taken())
            .for_each(|(_, t)| push(t.id));
        let week = {
            let days = (Self::today() - plan.created_on).num_days().max(0);
            (days / 7) as u32 + 1
        };
        plan.topics()
            .filter(|(_, t)| t.week <= week)
            .for_each(|(_, t)| push(t.id));
        plan.topics().for_each(|(_, t)| push(t.id));
        out.truncate(SUGGESTED_TOPICS);
        out
    }

    pub(crate) fn start_attempt(&mut self, proposal: ProposalId) -> Result<(), EngineError> {
        if !self.discursive.proposals.iter().any(|p| p.id == proposal) {
            return Err(EngineError::NoActivePlan);
        }
        let id = self.store.insert_attempt(
            proposal,
            AttemptOrigin::Photos,
            None,
            AttemptStatus::Draft,
            now(),
        )?;
        self.refresh_discursive();
        self.open_writing_attempt(id)
    }

    pub(crate) fn open_writing_attempt(&mut self, id: AttemptId) -> Result<(), EngineError> {
        if !self.discursive.attempts.iter().any(|a| a.id == id) {
            self.refresh_discursive();
        }
        let open = load_open(&self.store, id)?;
        self.discursive.images.clear();
        if let Some(open) = &open {
            self.load_page_images(&open.pages);
        }
        self.discursive.open = open;
        Ok(())
    }

    fn load_page_images(&self, pages: &[Page]) {
        let paths: Vec<(PageId, PathBuf)> = pages
            .iter()
            .map(|p| (p.id, PathBuf::from(&p.processed_path)))
            .collect();
        if paths.is_empty() {
            return;
        }
        let (tx, waker) = self.sender();
        let spawned = std::thread::Builder::new()
            .name("kg-pages".into())
            .spawn(move || {
                for (page, path) in paths {
                    if let Ok(bytes) = std::fs::read(&path) {
                        let _ = tx.send(Event::Discursive(DiscursiveEvent::PageImage {
                            page,
                            bytes: bytes.into(),
                        }));
                        waker();
                    }
                }
            });
        if let Err(err) = spawned {
            tracing::error!(%err, "could not spawn page loader");
        }
    }

    /// The open attempt, when its pages can still change.
    fn editable_attempt(&self) -> Result<&OpenAttempt, EngineError> {
        self.discursive
            .open
            .as_ref()
            .filter(|o| o.attempt.status.pages_editable())
            .ok_or(EngineError::AttemptLocked)
    }

    pub(crate) fn add_pages(&mut self, paths: Vec<PathBuf>) -> Result<(), EngineError> {
        let open = self.editable_attempt()?;
        if open.pages.len() + self.discursive.adding + paths.len() > MAX_PAGES {
            return Err(EngineError::TooManyPages);
        }
        let attempt = open.attempt.id;
        self.discursive.adding += paths.len();
        self.store_photos(attempt, paths.into_iter().map(|p| (p, None)).collect());
        Ok(())
    }

    pub(crate) fn replace_page(&mut self, page: PageId, path: PathBuf) -> Result<(), EngineError> {
        let open = self.editable_attempt()?;
        if !open.pages.iter().any(|p| p.id == page) {
            return Err(EngineError::AttemptLocked);
        }
        let attempt = open.attempt.id;
        self.discursive.adding += 1;
        self.store_photos(attempt, vec![(path, Some(page))]);
        Ok(())
    }

    /// Reads, checks and keeps photos on a worker, in the order given.
    fn store_photos(&self, attempt: AttemptId, photos: Vec<(PathBuf, Option<PageId>)>) {
        let dir = images::attempt_dir(&self.paths.data_dir, attempt);
        let (tx, waker) = self.sender();
        let spawned = std::thread::Builder::new()
            .name("kg-photos".into())
            .spawn(move || {
                for (path, replace) in photos {
                    let result = images::read_photo(&path)
                        .and_then(|(bytes, format)| images::store_photo(&dir, &bytes, format));
                    let _ = tx.send(Event::Discursive(DiscursiveEvent::PageStored {
                        attempt,
                        replace,
                        result,
                    }));
                    waker();
                }
            });
        if let Err(err) = spawned {
            tracing::error!(%err, "could not spawn photo worker");
        }
    }

    pub(crate) fn rotate_page(&mut self, page: PageId) -> Result<(), EngineError> {
        let open = self.editable_attempt()?;
        let Some(current) = open.pages.iter().find(|p| p.id == page) else {
            return Ok(());
        };
        let original = PathBuf::from(&current.original_path);
        let rotation = (current.rotation + 1) % 4;
        self.discursive.adding += 1;
        self.spawn(move || {
            Event::Discursive(DiscursiveEvent::PageRotated {
                page,
                result: images::rotate_photo(&original, rotation),
            })
        });
        Ok(())
    }

    pub(crate) fn move_page(&mut self, page: PageId, up: bool) -> Result<(), EngineError> {
        self.editable_attempt()?;
        self.store.move_page(page, up)?;
        self.reload_open();
        Ok(())
    }

    pub(crate) fn remove_page(&mut self, page: PageId) -> Result<(), EngineError> {
        self.editable_attempt()?;
        if let Some((original, processed)) = self.store.remove_page(page)? {
            remove_files(&[original, processed]);
        }
        self.discursive.images.remove(&page);
        self.reload_open();
        Ok(())
    }

    /// Sends the pages to be read. Several clicks start one run.
    pub(crate) fn submit_pages(&mut self) -> Result<(), EngineError> {
        let open = self.editable_attempt()?;
        if open.pages.is_empty() || self.discursive.adding > 0 {
            return Err(EngineError::NoPages);
        }
        if open
            .pages
            .iter()
            .any(|p| p.issues.iter().any(|i| i.blocks()))
        {
            return Err(EngineError::PageNeedsRetake);
        }
        let attempt = open.attempt.id;
        self.start_transcription(attempt)
    }

    fn start_transcription(&mut self, attempt: AttemptId) -> Result<(), EngineError> {
        // Images go only to a vision model: no fallback to a text model.
        let client = self.ai_client();
        let model = client.config().models.vision.trim().to_owned();
        if model.is_empty() {
            return Err(AiError::NoVisionModel.into());
        }
        let pages: Vec<PathBuf> = self
            .store
            .pages(attempt)?
            .iter()
            .map(|p| PathBuf::from(&p.processed_path))
            .collect();
        if pages.is_empty() {
            return Err(EngineError::NoPages);
        }
        let Some(run) = self
            .store
            .begin_run(attempt, AttemptStatus::Transcribing, now())?
        else {
            return Ok(());
        };
        tracing::info!(
            attempt = attempt.0,
            run,
            pages = pages.len(),
            "transcribing pages"
        );
        self.reload_open();
        self.refresh_discursive();
        self.spawn(move || {
            let total = pages.len();
            let result = pages
                .iter()
                .enumerate()
                .map(|(i, path)| {
                    let jpeg = std::fs::read(path).map_err(|source| EngineError::Export {
                        path: path.clone(),
                        source,
                    })?;
                    let reading = kg_ai::read_manuscript_page(&client, &jpeg, i + 1, total)?;
                    Ok(validate::page(reading, i as u32 + 1))
                })
                .collect();
            Event::Discursive(DiscursiveEvent::Transcribed {
                attempt,
                run,
                model,
                result,
            })
        });
        Ok(())
    }

    /// Saves the student's check of the transcript and grades it. A check
    /// that changes too much is a rewrite and is refused here.
    pub(crate) fn confirm_transcript(&mut self, pages: Vec<String>) -> Result<(), EngineError> {
        let (attempt, original) = self.checkable_transcript(&pages)?;
        if change_ratio(&original, &pages) > REWRITE_RATIO {
            return Err(EngineError::LooksLikeRewrite);
        }
        let confirmed = ConfirmedTranscript {
            edits: edits(&original, &pages),
            pages,
        };
        self.store.confirm_transcript(attempt, &confirmed, now())?;
        self.start_evaluation(attempt)
    }

    /// The open attempt's original transcript, when it can be checked with
    /// `pages` (one text per page).
    fn checkable_transcript(
        &self,
        pages: &[String],
    ) -> Result<(AttemptId, Transcript), EngineError> {
        let open = self
            .discursive
            .open
            .as_ref()
            .filter(|o| {
                matches!(
                    o.attempt.status,
                    AttemptStatus::Review | AttemptStatus::Pending
                )
            })
            .ok_or(EngineError::AttemptLocked)?;
        let record = open.transcript.as_ref().ok_or(EngineError::AttemptLocked)?;
        if record.original.pages.len() != pages.len() {
            return Err(EngineError::AttemptLocked);
        }
        Ok((open.attempt.id, record.original.clone()))
    }

    /// Saves a rewritten text as a new attempt on the same proposal and
    /// grades it; the attempt it came from stays as it was.
    pub(crate) fn save_rewrite(&mut self, pages: Vec<String>) -> Result<(), EngineError> {
        let (parent, _) = self.checkable_transcript(&pages)?;
        let proposal = self
            .discursive
            .open
            .as_ref()
            .map(|o| o.proposal.id)
            .ok_or(EngineError::AttemptLocked)?;
        let id = self.store.insert_attempt(
            proposal,
            AttemptOrigin::Rewrite,
            Some(parent),
            AttemptStatus::Review,
            now(),
        )?;
        let typed = Transcript {
            pages: pages
                .iter()
                .enumerate()
                .map(|(i, text)| TranscriptPage {
                    page: i as u32 + 1,
                    lines: text.lines().map(str::to_owned).collect(),
                    uncertain: Vec::new(),
                    legibility: Legibility::Good,
                    warnings: Vec::new(),
                    retake: None,
                })
                .collect(),
        };
        self.store.save_transcript(id, &typed, "", "typed", now())?;
        self.store.confirm_transcript(
            id,
            &ConfirmedTranscript {
                pages,
                edits: Vec::new(),
            },
            now(),
        )?;
        self.refresh_discursive();
        self.open_writing_attempt(id)?;
        self.start_evaluation(id)
    }

    /// Grades a confirmed transcript, or leaves it pending when too little
    /// of it was read for a fair grade.
    fn start_evaluation(&mut self, attempt: AttemptId) -> Result<(), EngineError> {
        let saved = self
            .store
            .attempt(attempt)?
            .ok_or(EngineError::AttemptLocked)?;
        let proposal = self
            .store
            .proposal(saved.proposal_id)?
            .ok_or(EngineError::AttemptLocked)?;
        let pages = self
            .store
            .transcript(attempt)?
            .and_then(|t| t.confirmed)
            .map(|c| c.pages)
            .ok_or(EngineError::AttemptLocked)?;
        if !Unreadable::of(&pages).fair_to_grade() {
            self.store.set_attempt_status(
                attempt,
                AttemptStatus::Pending,
                None,
                NOTE_UNREADABLE,
                now(),
            )?;
            self.after_change();
            return Ok(());
        }
        let Some(run) = self
            .store
            .begin_run(attempt, AttemptStatus::Evaluating, now())?
        else {
            return Ok(());
        };
        let plan = self.active.as_ref().ok_or(EngineError::NoActivePlan)?;
        let topics: Vec<TopicId> = plan.topics().map(|(_, t)| t.id).collect();
        let input = EvaluationInput {
            kind: proposal.kind.prompt_name().to_owned(),
            theme: proposal.theme.clone(),
            statement: proposal.statement.clone(),
            instructions: proposal.instructions.clone(),
            line_limit: proposal.line_limit,
            criteria: proposal
                .rubric
                .criteria
                .iter()
                .map(|c| kg_ai::CriterionInput {
                    key: c.key.clone(),
                    name: c.name.clone(),
                    description: c.description.clone(),
                    max_points: c.max_points,
                })
                .collect(),
            expected_points: proposal.expected_points.clone(),
            topics: plan
                .topics()
                .map(|(s, t)| format!("{} — {}", s.name, t.title))
                .collect(),
            pages: pages.clone(),
        };
        let client = self.ai_client();
        let model = client.config().models.correction.clone();
        tracing::info!(attempt = attempt.0, run, "evaluating written answer");
        self.after_change();
        let rubric = proposal.rubric;
        self.spawn(move || {
            let mut result = Err(EngineError::Ai(AiError::InvalidResponse(String::new())));
            for try_number in 1..=2 {
                result = kg_ai::evaluate_writing(&client, &input)
                    .map_err(EngineError::from)
                    .and_then(|draft| {
                        validate::evaluation(draft, &rubric, &pages, &topics, &model, now())
                            .map_err(|invalid| {
                                tracing::warn!(try_number, reason = %invalid.0, "evaluation refused");
                                EngineError::Ai(AiError::InvalidResponse(invalid.0))
                            })
                    });
                if !matches!(result, Err(EngineError::Ai(AiError::InvalidResponse(_)))) {
                    break;
                }
            }
            Event::Discursive(DiscursiveEvent::Evaluated {
                attempt,
                run,
                result,
            })
        });
        Ok(())
    }

    /// Runs again the step that failed.
    pub(crate) fn retry_attempt(&mut self) -> Result<(), EngineError> {
        let open = self
            .discursive
            .open
            .as_ref()
            .filter(|o| o.attempt.status == AttemptStatus::Failed)
            .ok_or(EngineError::AttemptLocked)?;
        let attempt = open.attempt.id;
        match open.attempt.failed_stage {
            Some(Stage::Evaluation) => self.start_evaluation(attempt),
            _ => self.start_transcription(attempt),
        }
    }

    /// Back to checking the transcript of a pending attempt.
    pub(crate) fn reopen_transcript(&mut self) -> Result<(), EngineError> {
        let open = self
            .discursive
            .open
            .as_ref()
            .filter(|o| o.attempt.status == AttemptStatus::Pending && o.transcript.is_some())
            .ok_or(EngineError::AttemptLocked)?;
        let attempt = open.attempt.id;
        self.store
            .set_attempt_status(attempt, AttemptStatus::Review, None, "", now())?;
        self.after_change();
        Ok(())
    }

    pub(crate) fn delete_attempt(&mut self, id: AttemptId) -> Result<(), EngineError> {
        self.store.delete_attempt(id)?;
        self.remove_attempt_files(id);
        if self
            .discursive
            .open
            .as_ref()
            .is_some_and(|o| o.attempt.id == id)
        {
            self.close_attempt();
        }
        self.refresh_discursive();
        Ok(())
    }

    pub(crate) fn delete_proposal(&mut self, id: ProposalId) -> Result<(), EngineError> {
        for attempt in self.store.delete_proposal(id)? {
            self.remove_attempt_files(attempt);
        }
        if self
            .discursive
            .open
            .as_ref()
            .is_some_and(|o| o.proposal.id == id)
        {
            self.close_attempt();
        }
        self.refresh_discursive();
        Ok(())
    }

    fn remove_attempt_files(&self, attempt: AttemptId) {
        let dir = images::attempt_dir(&self.paths.data_dir, attempt);
        if dir.exists()
            && let Err(err) = std::fs::remove_dir_all(&dir)
        {
            tracing::warn!(%err, attempt = attempt.0, "could not remove attempt photos");
        }
    }

    /// Looks for difficulties repeated across graded attempts.
    pub(crate) fn analyze_writing(&mut self) -> Result<(), EngineError> {
        let plan = self.active.as_ref().ok_or(EngineError::NoActivePlan)?.id;
        if self.discursive.analyzing {
            return Ok(());
        }
        let mut attempts = Vec::new();
        let mut items = Vec::new();
        for (id, eval) in self
            .discursive
            .evaluations
            .iter()
            .rev()
            .take(ISSUE_ATTEMPTS)
            .rev()
        {
            let Some(pages) = self.store.transcript(*id)?.and_then(|t| t.confirmed) else {
                continue;
            };
            attempts.push((*id, pages.pages));
            items.push(IssueInput {
                attempt: attempts.len(),
                lines: eval
                    .problems
                    .iter()
                    .map(|p| {
                        let category = match p.category {
                            ProblemCategory::Writing => "escrita",
                            ProblemCategory::Content => "conteudo",
                            ProblemCategory::Structure => "estrutura",
                        };
                        let quote = p.evidence.as_ref().map_or("", |e| e.quote.as_str());
                        format!("{category} — {} — {quote}", p.text)
                    })
                    .collect(),
            });
        }
        if attempts.len() < 2 {
            return Err(EngineError::NotEnoughAttempts);
        }
        let client = self.ai_client();
        let model = client.config().models.correction.clone();
        self.discursive.analyzing = true;
        self.spawn(move || {
            let result = kg_ai::find_recurring_issues(&client, &items);
            Event::Discursive(DiscursiveEvent::Issues {
                plan,
                attempts,
                model,
                result,
            })
        });
        Ok(())
    }

    /// Sends a drawn number to the vision model and checks it reads it.
    pub(crate) fn check_vision(&mut self) -> Result<(), EngineError> {
        if self.discursive.vision == VisionCheck::Checking {
            return Ok(());
        }
        if self.vision_target().is_none() {
            return Err(AiError::NoVisionModel.into());
        }
        let (png, number) = images::vision_probe()?;
        let client = self.ai_client();
        self.discursive.vision = VisionCheck::Checking;
        self.spawn(move || {
            Event::Discursive(DiscursiveEvent::VisionChecked(kg_ai::check_vision(
                &client, &png, &number,
            )))
        });
        Ok(())
    }

    pub(crate) fn on_discursive_event(
        &mut self,
        event: DiscursiveEvent,
    ) -> Result<(), EngineError> {
        match event {
            DiscursiveEvent::Proposal {
                plan,
                request,
                candidates,
                edital,
                model,
                result,
            } => {
                self.discursive.generating = false;
                if self.discursive.plan != Some(plan) {
                    return Ok(());
                }
                let draft = (*result)?;
                let proposal = validate::proposal(draft, &request, &candidates, &edital, &model)
                    .map_err(|invalid| AiError::InvalidResponse(invalid.0))?;
                self.store.insert_proposal(plan, &proposal, now())?;
                self.refresh_discursive();
                self.notices.push(Notice::ProposalCreated);
                Ok(())
            }
            DiscursiveEvent::PageStored {
                attempt,
                replace,
                result,
            } => {
                self.discursive.adding = self.discursive.adding.saturating_sub(1);
                let prepared = result?;
                let new = &prepared.page;
                let current = self.store.attempt(attempt)?;
                let fits = self.store.pages(attempt)?.len() < MAX_PAGES || replace.is_some();
                if !current.is_some_and(|a| a.status.pages_editable()) || !fits {
                    remove_files(&[new.original_path.clone(), new.processed_path.clone()]);
                    return Ok(());
                }
                let page = match replace {
                    Some(page) => {
                        let old = self
                            .store
                            .pages(attempt)?
                            .into_iter()
                            .find(|p| p.id == page);
                        let Some(old) = old else {
                            remove_files(&[new.original_path.clone(), new.processed_path.clone()]);
                            return Ok(());
                        };
                        self.store.update_page(page, new)?;
                        remove_files(&[old.original_path, old.processed_path]);
                        page
                    }
                    None => self.store.add_page(attempt, new)?,
                };
                self.put_image(page, prepared.jpeg.into());
                self.reload_open();
                Ok(())
            }
            DiscursiveEvent::PageRotated { page, result } => {
                self.discursive.adding = self.discursive.adding.saturating_sub(1);
                let prepared = result?;
                let old = self
                    .discursive
                    .open
                    .as_ref()
                    .filter(|o| o.attempt.status.pages_editable())
                    .and_then(|o| o.pages.iter().find(|p| p.id == page).cloned());
                let Some(old) = old else {
                    remove_files(&[prepared.page.processed_path]);
                    return Ok(());
                };
                self.store.update_page(page, &prepared.page)?;
                remove_files(&[old.processed_path]);
                self.put_image(page, prepared.jpeg.into());
                self.reload_open();
                Ok(())
            }
            DiscursiveEvent::PageImage { page, bytes } => {
                if self
                    .discursive
                    .open
                    .as_ref()
                    .is_some_and(|o| o.pages.iter().any(|p| p.id == page))
                {
                    self.put_image(page, bytes);
                }
                Ok(())
            }
            DiscursiveEvent::Transcribed {
                attempt,
                run,
                model,
                result,
            } => {
                if !self.is_current_run(attempt, run, AttemptStatus::Transcribing)? {
                    return Ok(());
                }
                match result {
                    Ok(readings) => {
                        let pages = self.store.pages(attempt)?;
                        let mut transcript = Transcript::default();
                        for (reading, issues) in readings {
                            if let Some(page) = pages.get(reading.page as usize - 1)
                                && !issues.is_empty()
                            {
                                let mut updated = new_page(page);
                                for issue in issues {
                                    if !updated.issues.contains(&issue) {
                                        updated.issues.push(issue);
                                    }
                                }
                                self.store.update_page(page.id, &updated)?;
                            }
                            transcript.pages.push(reading);
                        }
                        self.store.save_transcript(
                            attempt,
                            &transcript,
                            &model,
                            &kg_ai::prompt_version("transcribe"),
                            now(),
                        )?;
                        let retakes = transcript.retakes();
                        if retakes.is_empty() {
                            self.store.set_attempt_status(
                                attempt,
                                AttemptStatus::Review,
                                None,
                                "",
                                now(),
                            )?;
                        } else {
                            let note = retakes
                                .iter()
                                .map(|(page, reason)| format!("{page}: {reason}"))
                                .collect::<Vec<_>>()
                                .join("\n");
                            self.store.set_attempt_status(
                                attempt,
                                AttemptStatus::Retake,
                                None,
                                &note,
                                now(),
                            )?;
                        }
                        self.after_change();
                        Ok(())
                    }
                    Err(err) => {
                        self.fail(attempt, Stage::Transcription, &err)?;
                        Err(err)
                    }
                }
            }
            DiscursiveEvent::Evaluated {
                attempt,
                run,
                result,
            } => {
                if !self.is_current_run(attempt, run, AttemptStatus::Evaluating)? {
                    return Ok(());
                }
                match result {
                    Ok(validate::Outcome::Graded(eval)) => {
                        self.store.save_evaluation(attempt, &eval)?;
                        self.store.set_attempt_status(
                            attempt,
                            AttemptStatus::Evaluated,
                            None,
                            "",
                            now(),
                        )?;
                        self.after_change();
                        self.notices.push(Notice::WritingEvaluated {
                            total: eval.total(),
                            max: eval.max_total(),
                        });
                        Ok(())
                    }
                    Ok(validate::Outcome::Pending(reason)) => {
                        let note = if reason.is_empty() {
                            NOTE_UNREADABLE.to_owned()
                        } else {
                            reason
                        };
                        self.store.set_attempt_status(
                            attempt,
                            AttemptStatus::Pending,
                            None,
                            &note,
                            now(),
                        )?;
                        self.after_change();
                        Ok(())
                    }
                    Err(err) => {
                        self.fail(attempt, Stage::Evaluation, &err)?;
                        Err(err)
                    }
                }
            }
            DiscursiveEvent::Issues {
                plan,
                attempts,
                model,
                result,
            } => {
                self.discursive.analyzing = false;
                let drafts = result?;
                if self.discursive.plan != Some(plan) {
                    return Ok(());
                }
                let issues: Vec<RecurringIssue> = validate::issues(drafts, &attempts);
                let report = DifficultyReport {
                    issues,
                    attempts: attempts.iter().map(|(id, _)| *id).collect(),
                    model,
                    prompt_version: kg_ai::prompt_version("difficulties"),
                    created_at: now(),
                };
                self.store.save_difficulty_report(plan, &report)?;
                self.discursive.report = Some(report);
                Ok(())
            }
            DiscursiveEvent::VisionChecked(result) => match result {
                Ok(true) => {
                    self.discursive.vision = VisionCheck::Works;
                    Ok(())
                }
                Ok(false) => {
                    self.discursive.vision = VisionCheck::NotReading;
                    Ok(())
                }
                Err(err) => {
                    self.discursive.vision = VisionCheck::Unknown;
                    Err(err.into())
                }
            },
        }
    }

    /// True when `run` is the attempt's latest run and it is still in
    /// `status`; results of older runs or deleted attempts are dropped.
    fn is_current_run(
        &self,
        attempt: AttemptId,
        run: u32,
        status: AttemptStatus,
    ) -> Result<bool, EngineError> {
        let current = self.store.attempt(attempt)?;
        let applies = current.is_some_and(|a| a.run == run && a.status == status);
        if !applies {
            tracing::info!(attempt = attempt.0, run, "stale result dropped");
        }
        Ok(applies)
    }

    fn fail(
        &mut self,
        attempt: AttemptId,
        stage: Stage,
        err: &EngineError,
    ) -> Result<(), EngineError> {
        tracing::warn!(attempt = attempt.0, stage = stage.as_str(), %err, "written answer step failed");
        self.store.set_attempt_status(
            attempt,
            AttemptStatus::Failed,
            Some(stage),
            &err.to_string(),
            now(),
        )?;
        self.after_change();
        Ok(())
    }

    fn put_image(&mut self, page: PageId, bytes: Arc<[u8]>) {
        self.discursive.image_version += 1;
        let version = self.discursive.image_version;
        self.discursive.images.insert(page, (version, bytes));
    }

    fn reload_open(&mut self) {
        let Some(id) = self.discursive.open.as_ref().map(|o| o.attempt.id) else {
            return;
        };
        match load_open(&self.store, id) {
            Ok(open) => self.discursive.open = open,
            Err(err) => self.notices.push(Notice::Error(err)),
        }
    }

    fn after_change(&mut self) {
        self.refresh_discursive();
        self.reload_open();
    }

    /// True while written-answer work runs in the background.
    pub(crate) fn writing_busy(&self) -> bool {
        let d = &self.discursive;
        d.generating
            || d.analyzing
            || d.adding > 0
            || d.vision == VisionCheck::Checking
            || d.attempts.iter().any(|a| a.status.is_processing())
    }
}

fn load_open(store: &kg_study::Store, id: AttemptId) -> Result<Option<OpenAttempt>, EngineError> {
    let Some(attempt) = store.attempt(id)? else {
        return Ok(None);
    };
    let Some(proposal) = store.proposal(attempt.proposal_id)? else {
        return Ok(None);
    };
    Ok(Some(OpenAttempt {
        pages: store.pages(id)?,
        transcript: store.transcript(id)?,
        evaluation: store.evaluation(id)?,
        attempt,
        proposal,
    }))
}

fn new_page(page: &Page) -> kg_study::NewPage {
    kg_study::NewPage {
        original_path: page.original_path.clone(),
        processed_path: page.processed_path.clone(),
        rotation: page.rotation,
        width: page.width,
        height: page.height,
        issues: page.issues.clone(),
    }
}

fn remove_files(paths: &[String]) {
    for path in paths {
        let path = Path::new(path);
        if path.exists()
            && let Err(err) = std::fs::remove_file(path)
        {
            tracing::warn!(%err, "could not remove a page file");
        }
    }
}

/// The host part of a URL, to tell where images go.
fn host(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split('/').next().unwrap_or(rest).to_owned()
}

#[cfg(test)]
mod tests;
