//! Mock exams (simulados): the exam's format from the notice, questions
//! written subject by subject, a printable PDF, timed attempts and their
//! grade. The AI writes questions and reads the format; everything else
//! (the clock, the key, the score) is plain code.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use chrono::NaiveDateTime;
use kg_ai::{AiError, ExamQuestionDraft, ExamQuestionsInput, FormatDraft, ProposalDraft};
use kg_handout::{
    CardLayout, EssaySheet, ExamDocument, ExamLabels, ExamPageQuestion, ExamPageSection,
};
use kg_study::mock::{MAX_MINUTES, MAX_QUESTIONS, MIN_MINUTES, spread};
use kg_study::{
    AnswerStyle, Difficulty, ExamFormat, ExamSection, FormatSource, MockAttempt, MockAttemptId,
    MockExam, MockExamId, MockQuestion, MockResult, NewMockExam, PlanId, ProposalId, QuizQuestion,
    TopicId,
};

use crate::discursive::ProposalRequest;
use crate::{Engine, EngineError, Event, Notice};

/// Questions asked of the model in one request.
const BATCH_QUESTIONS: u32 = 10;
/// Requests running at the same time.
const PARALLEL_BATCHES: usize = 3;
/// Board material sent with one request, at most.
const REFERENCE_CHARS: usize = 6_000;
/// Notice text sent to read the format, at most.
const RULES_CHARS: usize = 60_000;
/// Lines on the essay sheet when the proposal sets no limit.
const ESSAY_LINES: u32 = 30;

/// A mock exam being written.
#[derive(Debug)]
pub struct MockJob {
    pub total: u32,
    done: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
}

impl MockJob {
    /// Questions written so far.
    pub fn done(&self) -> u32 {
        self.done.load(Ordering::Relaxed)
    }
}

/// What the UI sends to print a mock exam: the words of the PDF in the
/// app's language and where to save it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockPrint {
    pub exam: MockExamId,
    pub path: PathBuf,
    pub subtitle: String,
    pub lang: String,
    pub labels: ExamLabels,
    /// Already formatted, e.g. "4 h 30 min".
    pub duration: String,
    pub instructions: Vec<String>,
    /// Letters for options ("A", "B"…) and for true/false ("C", "E").
    pub choice_marks: Vec<String>,
    pub true_false_marks: [String; 2],
}

#[derive(Debug, Default)]
pub(crate) struct MockState {
    plan: Option<PlanId>,
    saved_format: Option<ExamFormat>,
    exams: Vec<MockExam>,
    attempts: Vec<MockAttempt>,
    open: Option<MockAttemptId>,
    reading_format: bool,
    printing: bool,
    job: Option<MockJob>,
}

/// The essay part of a mock exam being written.
pub(crate) struct EssayJob {
    request: ProposalRequest,
    candidates: Vec<TopicId>,
    edital: String,
    model: String,
    result: Result<ProposalDraft, AiError>,
}

pub(crate) enum MockEvent {
    Format {
        plan: PlanId,
        rules: String,
        result: Result<FormatDraft, AiError>,
    },
    Written {
        plan: PlanId,
        title: String,
        format: ExamFormat,
        model: String,
        result: Result<Vec<MockQuestion>, EngineError>,
        essay: Option<Box<EssayJob>>,
    },
    Printed(Result<PathBuf, EngineError>),
}

/// One request to the model: some topics of one section.
struct Batch {
    section: usize,
    input: ExamQuestionsInput,
    topics: Vec<TopicId>,
}

impl Engine {
    /// The format mock exams follow: the one saved for the plan, or an
    /// estimate from its subjects.
    pub fn exam_format(&self) -> Option<ExamFormat> {
        if let Some(format) = &self.mock.saved_format {
            return Some(format.clone());
        }
        let plan = self.active.as_ref()?;
        let subjects: Vec<String> = plan.subjects.iter().map(|s| s.name.clone()).collect();
        Some(estimate_format(
            &subjects,
            self.default_options(),
            self.mock_writing_kind().is_some(),
        ))
    }

    /// The exam asks Certo/Errado items: its saved format says so, the plan
    /// names Cebraspe, or most of its past exam questions are true/false.
    pub(crate) fn exam_is_true_false(&self) -> bool {
        if let Some(format) = &self.mock.saved_format {
            return format.style == AnswerStyle::TrueFalse;
        }
        let Some(plan) = self.active.as_ref() else {
            return false;
        };
        let names = format!("{} {}", plan.exam, plan.title).to_lowercase();
        if names.contains("cebraspe") || names.contains("cespe") {
            return true;
        }
        let answered: Vec<_> = self
            .board
            .questions
            .iter()
            .filter(|q| q.correct.is_some())
            .collect();
        let true_false = answered
            .iter()
            .filter(|q| kg_study::is_true_false(&q.options))
            .count();
        true_false * 2 > answered.len()
    }

    pub fn reading_exam_format(&self) -> bool {
        self.mock.reading_format
    }

    /// Mock exams of the active plan, newest first.
    pub fn mock_exams(&self) -> &[MockExam] {
        &self.mock.exams
    }

    /// Attempts on the active plan's mock exams, newest first.
    pub fn mock_attempts(&self) -> &[MockAttempt] {
        &self.mock.attempts
    }

    pub fn mock_job(&self) -> Option<&MockJob> {
        self.mock.job.as_ref()
    }

    pub fn printing_mock(&self) -> bool {
        self.mock.printing
    }

    /// The attempt on screen and its exam.
    pub fn open_mock(&self) -> Option<(&MockExam, &MockAttempt)> {
        let attempt = self
            .mock
            .attempts
            .iter()
            .find(|a| Some(a.id) == self.mock.open)?;
        let exam = self.mock.exams.iter().find(|e| e.id == attempt.exam_id)?;
        Some((exam, attempt))
    }

    /// An attempt whose clock is still running, if any.
    pub fn running_mock(&self) -> Option<&MockAttempt> {
        self.mock.attempts.iter().find(|a| a.finished_at.is_none())
    }

    /// The grade of a handed-in attempt.
    pub fn mock_result(&self, attempt: &MockAttempt) -> Option<MockResult> {
        attempt.finished_at?;
        let exam = self.mock.exams.iter().find(|e| e.id == attempt.exam_id)?;
        Some(kg_study::mock::grade(exam, &attempt.answers))
    }

    pub(crate) fn mock_sitting(&self) -> bool {
        self.open_mock()
            .is_some_and(|(_, attempt)| attempt.finished_at.is_none())
    }

    pub(crate) fn refresh_mock(&mut self) {
        let plan = self.active.as_ref().map(|p| p.id);
        if plan != self.mock.plan {
            self.mock.open = None;
            self.cancel_mock();
        }
        self.mock.plan = plan;
        let Some(plan) = plan else {
            self.mock.saved_format = None;
            self.mock.exams.clear();
            self.mock.attempts.clear();
            return;
        };
        let loaded = (|| -> Result<_, EngineError> {
            Ok((
                self.store.exam_format(plan)?,
                self.store.mock_exams(plan)?,
                self.store.mock_attempts(plan)?,
            ))
        })();
        match loaded {
            Ok((format, exams, attempts)) => {
                self.mock.saved_format = format;
                self.mock.exams = exams;
                self.mock.attempts = attempts;
            }
            Err(err) => self.notices.push(Notice::Error(err)),
        }
        if self
            .mock
            .open
            .is_some_and(|id| !self.mock.attempts.iter().any(|a| a.id == id))
        {
            self.mock.open = None;
        }
    }

    /// Asks the model how the notice lays out the exam.
    pub(crate) fn read_exam_format(&mut self) -> Result<(), EngineError> {
        if self.mock.reading_format {
            return Ok(());
        }
        let plan = self.active.as_ref().ok_or(EngineError::NoActivePlan)?;
        let text = self.store.edital_text(plan.id)?;
        let rules = kg_pdf::EditalText::from_raw(&text).exam_rules(RULES_CHARS);
        let subjects: Vec<String> = plan.subjects.iter().map(|s| s.name.clone()).collect();
        let role = plan.exam.clone();
        let plan_id = plan.id;
        let client = self.ai_client();
        self.mock.reading_format = true;
        self.spawn(move || {
            let result = kg_ai::read_exam_format(&client, &rules, &role, &subjects);
            Event::Mock(MockEvent::Format {
                plan: plan_id,
                rules,
                result,
            })
        });
        Ok(())
    }

    pub(crate) fn save_exam_format(&mut self, mut format: ExamFormat) -> Result<(), EngineError> {
        let plan = self.active.as_ref().ok_or(EngineError::NoActivePlan)?.id;
        format.validate()?;
        format.source = FormatSource::Student;
        self.store.save_exam_format(plan, &format)?;
        self.mock.saved_format = Some(format);
        Ok(())
    }

    /// Writes a new mock exam in the background, following the format.
    pub(crate) fn generate_mock(&mut self, title: String) -> Result<(), EngineError> {
        if self.mock.job.is_some() {
            return Ok(());
        }
        let format = self.exam_format().ok_or(EngineError::NoActivePlan)?;
        format.validate()?;
        let plan = self.active.as_ref().ok_or(EngineError::NoActivePlan)?;
        let exam_name = if plan.exam.is_empty() {
            plan.title.clone()
        } else {
            plan.exam.clone()
        };
        // Later exams start further down each subject's topics, so a short
        // section covers different topics each time.
        let offset = self.mock.exams.len();
        let options = match format.style {
            AnswerStyle::Choice { options } => Some(u32::from(options)),
            AnswerStyle::TrueFalse => None,
        };
        let mut batches = Vec::new();
        for (index, section) in format.used_sections() {
            let subject = plan
                .subjects
                .iter()
                .find(|s| same_subject(&s.name, &section.subject));
            let topics: Vec<(TopicId, String)> = subject
                .map(|s| s.topics.iter().map(|t| (t.id, t.title.clone())).collect())
                .unwrap_or_default();
            if topics.is_empty() {
                return Err(EngineError::MockSubject(section.subject.clone()));
            }
            let mut rotated = topics.clone();
            rotated.rotate_left(offset % topics.len());
            let counts = spread(section.questions, rotated.len());
            let wanted: Vec<(TopicId, String, u32)> = rotated
                .into_iter()
                .zip(counts)
                .filter(|(_, n)| *n > 0)
                .map(|((id, title), n)| (id, title, n))
                .collect();
            for group in batch_topics(&wanted, BATCH_QUESTIONS) {
                let mut reference = String::new();
                for (id, _, _) in &group {
                    let block = self.board_reference(*id);
                    if reference.len() + block.len() <= REFERENCE_CHARS {
                        reference.push_str(&block);
                    }
                }
                batches.push(Batch {
                    section: index,
                    input: ExamQuestionsInput {
                        exam: exam_name.clone(),
                        subject: section.subject.clone(),
                        topics: group.iter().map(|(_, t, n)| (t.clone(), *n)).collect(),
                        reference,
                        options,
                    },
                    topics: group.iter().map(|(id, _, _)| *id).collect(),
                });
            }
        }
        let essay = if format.essay {
            self.mock_essay_request()
        } else {
            None
        };
        let plan_id = plan.id;
        let client = self.ai_client();
        let model = client.config().models.quiz.clone();
        let total = format.total_questions();
        let done = Arc::new(AtomicU32::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        self.mock.job = Some(MockJob {
            total,
            done: Arc::clone(&done),
            cancel: Arc::clone(&cancel),
        });
        let seed = chrono::Local::now().timestamp() as u64;
        tracing::info!(
            questions = total,
            batches = batches.len(),
            "mock exam requested"
        );
        let spawned = self.spawn(move || {
            let result = write_questions(&client, batches, &format, seed, &done, &cancel);
            let essay = match (&result, essay) {
                (Ok(_), Some((request, input, candidates))) if !cancel.load(Ordering::Relaxed) => {
                    let result = kg_ai::generate_proposal(&client, &input);
                    Some(Box::new(EssayJob {
                        request,
                        candidates,
                        edital: input.edital,
                        model: model.clone(),
                        result,
                    }))
                }
                _ => None,
            };
            Event::Mock(MockEvent::Written {
                plan: plan_id,
                title,
                format,
                model,
                result,
                essay,
            })
        });
        if !spawned {
            self.mock.job = None;
        }
        Ok(())
    }

    fn mock_essay_request(&self) -> Option<(ProposalRequest, kg_ai::ProposalInput, Vec<TopicId>)> {
        let kind = self.mock_writing_kind()?;
        let request = ProposalRequest {
            kind,
            difficulty: Difficulty::Medium,
            topics: Vec::new(),
            manual_rubric: None,
            line_limit: None,
            minutes: None,
        };
        match self.proposal_input(&request) {
            Ok((input, candidates)) => Some((request, input, candidates)),
            Err(error) => {
                tracing::warn!(%error, "mock exam goes without its essay");
                None
            }
        }
    }

    pub(crate) fn cancel_mock(&mut self) {
        if let Some(job) = self.mock.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn delete_mock(&mut self, id: MockExamId) -> Result<(), EngineError> {
        if self.open_mock().is_some_and(|(exam, _)| exam.id == id) {
            self.mock.open = None;
        }
        self.store.delete_mock_exam(id)?;
        self.refresh_mock();
        Ok(())
    }

    /// Starts the clock on an exam. An attempt already running is opened
    /// instead: one sitting at a time.
    pub(crate) fn start_mock(&mut self, id: MockExamId) -> Result<(), EngineError> {
        if let Some(running) = self.running_mock() {
            self.mock.open = Some(running.id);
            return Ok(());
        }
        let exam = self
            .mock
            .exams
            .iter()
            .find(|e| e.id == id)
            .ok_or(EngineError::NoActivePlan)?;
        let attempt = self
            .store
            .start_mock_attempt(id, exam.questions.len(), now())?;
        self.mock.open = Some(attempt.id);
        self.mock.attempts.insert(0, attempt);
        Ok(())
    }

    pub(crate) fn open_mock_attempt(&mut self, id: MockAttemptId) {
        if self.mock.attempts.iter().any(|a| a.id == id) {
            self.mock.open = Some(id);
        }
    }

    pub(crate) fn close_mock(&mut self) {
        self.mock.open = None;
    }

    /// Marks (or clears) one answer of the running attempt and saves it.
    pub(crate) fn mark_mock_answer(
        &mut self,
        question: usize,
        answer: Option<u8>,
    ) -> Result<(), EngineError> {
        let Some(id) = self.mock.open else {
            return Ok(());
        };
        let Some(attempt) = self
            .mock
            .attempts
            .iter_mut()
            .find(|a| a.id == id && a.finished_at.is_none())
        else {
            return Ok(());
        };
        let Some(slot) = attempt.answers.get_mut(question) else {
            return Ok(());
        };
        *slot = answer;
        self.store.save_mock_answers(id, &attempt.answers)?;
        Ok(())
    }

    /// Hands the running attempt in: the clock stops, the grade is final
    /// and wrong answers go to the mistakes notebook.
    pub(crate) fn finish_mock(&mut self, true_false: [String; 2]) -> Result<(), EngineError> {
        let Some(id) = self.mock.open else {
            return Ok(());
        };
        let Some(attempt) = self
            .mock
            .attempts
            .iter_mut()
            .find(|a| a.id == id && a.finished_at.is_none())
        else {
            return Ok(());
        };
        let finished = now();
        attempt.finished_at = Some(finished);
        let answers = attempt.answers.clone();
        let exam_id = attempt.exam_id;
        self.store.finish_mock_attempt(id, &answers, finished)?;
        if let Some(exam) = self.mock.exams.iter().find(|e| e.id == exam_id) {
            let today = finished.date();
            for (question, answer) in exam.questions.iter().zip(&answers) {
                let (Some(topic), Some(answer)) = (question.topic, answer) else {
                    continue;
                };
                if *answer == question.correct {
                    continue;
                }
                let options = if question.options.is_empty() {
                    true_false.to_vec()
                } else {
                    question.options.clone()
                };
                let missed = QuizQuestion {
                    prompt: question.prompt.clone(),
                    options,
                    correct: usize::from(question.correct),
                    explanation: question.explanation.clone(),
                    from_board: false,
                };
                self.store
                    .record_mistake(topic, &missed, usize::from(*answer), today)?;
            }
        }
        self.refresh_stats();
        Ok(())
    }

    pub(crate) fn delete_mock_attempt(&mut self, id: MockAttemptId) -> Result<(), EngineError> {
        if self.mock.open == Some(id) {
            self.mock.open = None;
        }
        self.store.delete_mock_attempt(id)?;
        self.refresh_mock();
        Ok(())
    }

    /// Lays the exam out as a PDF and saves it, in the background.
    pub(crate) fn print_mock(&mut self, print: MockPrint) -> Result<(), EngineError> {
        if self.mock.printing {
            return Ok(());
        }
        let exam = self
            .mock
            .exams
            .iter()
            .find(|e| e.id == print.exam)
            .ok_or(EngineError::NoActivePlan)?;
        let proposal = exam.proposal.and_then(|id| {
            self.proposals()
                .iter()
                .find(|p| p.id == id)
                .map(|p| EssaySheet {
                    theme: p.theme.clone(),
                    statement: p.statement.clone(),
                    instructions: p.instructions.clone(),
                    lines: p.line_limit.unwrap_or(ESSAY_LINES).clamp(10, 90),
                })
        });
        let document = exam_document(exam, &print, proposal);
        let path = print.path;
        self.mock.printing = true;
        self.spawn(move || {
            let result = kg_handout::render_exam_pdf(&document)
                .map_err(EngineError::from)
                .and_then(|pdf| {
                    std::fs::write(&path, pdf).map_err(|source| EngineError::Export {
                        path: path.clone(),
                        source,
                    })
                })
                .map(|()| path);
            Event::Mock(MockEvent::Printed(result))
        });
        Ok(())
    }

    pub(crate) fn on_mock_event(&mut self, event: MockEvent) -> Result<(), EngineError> {
        match event {
            MockEvent::Format {
                plan,
                rules,
                result,
            } => {
                self.mock.reading_format = false;
                if self.mock.plan != Some(plan) {
                    return Ok(());
                }
                let draft = result?;
                let Some(active) = self.active.as_ref() else {
                    return Ok(());
                };
                let subjects: Vec<String> =
                    active.subjects.iter().map(|s| s.name.clone()).collect();
                let fallback = estimate_format(
                    &subjects,
                    self.default_options(),
                    self.mock_writing_kind().is_some(),
                );
                match format_from_draft(&draft, &subjects, &rules, &fallback) {
                    Some(format) => {
                        self.store.save_exam_format(plan, &format)?;
                        self.mock.saved_format = Some(format);
                        self.notices.push(Notice::ExamFormatRead { found: true });
                    }
                    None => self.notices.push(Notice::ExamFormatRead { found: false }),
                }
                Ok(())
            }
            MockEvent::Written {
                plan,
                title,
                format,
                model,
                result,
                essay,
            } => {
                let job = self.mock.job.take();
                if job.is_none() || self.mock.plan != Some(plan) {
                    return Ok(());
                }
                let questions = result?;
                let proposal = match essay {
                    Some(essay) => self.save_essay(plan, *essay),
                    None => None,
                };
                let exam = NewMockExam {
                    title,
                    format,
                    questions,
                    proposal,
                    model,
                };
                exam.validate()?;
                self.store.insert_mock_exam(plan, &exam, now())?;
                self.refresh_mock();
                self.notices.push(Notice::MockCreated);
                Ok(())
            }
            MockEvent::Printed(result) => {
                self.mock.printing = false;
                let path = result?;
                self.notices.push(Notice::Exported(path));
                Ok(())
            }
        }
    }

    /// Saves the essay of a new mock exam; a bad one is left out, with a
    /// notice, and the exam keeps its questions.
    fn save_essay(&mut self, plan: PlanId, essay: EssayJob) -> Option<ProposalId> {
        let saved = essay.result.map_err(EngineError::from).and_then(|draft| {
            self.save_mock_proposal(
                plan,
                draft,
                &essay.request,
                &essay.candidates,
                &essay.edital,
                &essay.model,
            )
        });
        match saved {
            Ok(id) => Some(id),
            Err(error) => {
                tracing::warn!(%error, "mock exam essay failed");
                self.notices.push(Notice::Error(error));
                None
            }
        }
    }

    /// Pretends a mock exam is being written, so a test can deliver it.
    #[cfg(test)]
    pub(crate) fn expect_mock(&mut self) {
        self.mock.job = Some(MockJob {
            total: 0,
            done: Arc::default(),
            cancel: Arc::default(),
        });
    }

    /// Options per question when nothing better is known: the region's
    /// usual number.
    fn default_options(&self) -> u8 {
        self.settings
            .region
            .and_then(crate::settings::Region::exam_culture)
            .map_or(4, |c| c.quiz_options())
            .clamp(2, 5) as u8
    }
}

fn now() -> NaiveDateTime {
    chrono::Local::now().naive_local()
}

/// A format from the plan's subjects alone: a few questions each, three
/// minutes a question, an hour more for the essay.
pub(crate) fn estimate_format(subjects: &[String], options: u8, essay: bool) -> ExamFormat {
    let per = (60 / subjects.len().max(1) as u32).clamp(2, 10);
    let total = per * subjects.len() as u32;
    let minutes = (total * 3 + if essay { 60 } else { 0 }).div_ceil(30) * 30;
    ExamFormat {
        minutes: minutes.clamp(MIN_MINUTES, MAX_MINUTES),
        style: AnswerStyle::Choice {
            options: options.clamp(2, 5),
        },
        wrong_cancels: false,
        sections: subjects
            .iter()
            .map(|s| ExamSection {
                subject: s.clone(),
                questions: per,
                weight: 1.0,
            })
            .collect(),
        essay,
        source: FormatSource::Estimate,
    }
}

/// Lowercase letters and digits, accents removed, for comparing names.
fn fold(text: &str) -> String {
    text.chars()
        .filter_map(|c| {
            let c = match c {
                'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' => 'a',
                'é' | 'ê' | 'è' | 'É' | 'Ê' => 'e',
                'í' | 'î' | 'Í' => 'i',
                'ó' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ô' | 'Õ' => 'o',
                'ú' | 'ü' | 'Ú' => 'u',
                'ç' | 'Ç' => 'c',
                other => other,
            };
            c.is_alphanumeric()
                .then(|| c.to_lowercase().next().unwrap_or(c))
        })
        .collect()
}

/// The model's subject name against a plan subject: equal, or one inside
/// the other once folded.
fn same_subject(plan: &str, other: &str) -> bool {
    let (a, b) = (fold(plan), fold(other));
    !a.is_empty() && !b.is_empty() && (a == b || a.contains(&b) || b.contains(&a))
}

/// Whole numbers written in `text`.
fn numbers_in(text: &str) -> BTreeSet<u32> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|n| !n.is_empty() && n.len() <= 4)
        .filter_map(|n| n.parse().ok())
        .collect()
}

/// Turns the model's reading into a format, or `None` when it found
/// nothing or its numbers are not in the notice.
pub(crate) fn format_from_draft(
    draft: &FormatDraft,
    subjects: &[String],
    rules: &str,
    fallback: &ExamFormat,
) -> Option<ExamFormat> {
    if !draft.found {
        return None;
    }
    let mut counts: HashMap<usize, (u32, f64)> = HashMap::new();
    for section in &draft.sections {
        if section.questions == 0 {
            continue;
        }
        let Some(index) = subjects
            .iter()
            .position(|s| same_subject(s, &section.subject))
        else {
            continue;
        };
        let weight = if section.weight.is_finite() && section.weight > 0.0 && section.weight <= 10.0
        {
            section.weight
        } else {
            1.0
        };
        let entry = counts.entry(index).or_insert((0, weight));
        entry.0 += section.questions;
    }
    if counts.is_empty() {
        return None;
    }
    // The model may split a block of questions among subjects, so not
    // every count is written in the notice; most must be, or the total.
    let numbers = numbers_in(rules);
    let total: u32 = counts.values().map(|(n, _)| n).sum();
    let seen = counts.values().filter(|(n, _)| numbers.contains(n)).count();
    if !numbers.contains(&total) && seen * 2 < counts.len() {
        return None;
    }
    if total > MAX_QUESTIONS {
        return None;
    }
    let minutes = if (MIN_MINUTES..=MAX_MINUTES).contains(&draft.minutes) {
        draft.minutes
    } else {
        fallback.minutes
    };
    let style = if draft.style.trim() == "true_false" {
        AnswerStyle::TrueFalse
    } else {
        match u8::try_from(draft.options) {
            Ok(options @ 2..=5) => AnswerStyle::Choice { options },
            _ => fallback.style,
        }
    };
    Some(ExamFormat {
        minutes,
        style,
        wrong_cancels: draft.wrong_cancels,
        sections: subjects
            .iter()
            .enumerate()
            .map(|(i, subject)| {
                let (questions, weight) = counts.get(&i).copied().unwrap_or((0, 1.0));
                ExamSection {
                    subject: subject.clone(),
                    questions,
                    weight,
                }
            })
            .collect(),
        essay: draft.essay && fallback.essay,
        source: FormatSource::Notice,
    })
}

/// Groups topics into requests of at most `max` questions, splitting a
/// topic that wants more.
fn batch_topics(wanted: &[(TopicId, String, u32)], max: u32) -> Vec<Vec<(TopicId, String, u32)>> {
    let mut batches: Vec<Vec<(TopicId, String, u32)>> = Vec::new();
    let mut current: Vec<(TopicId, String, u32)> = Vec::new();
    let mut size = 0;
    for (id, title, count) in wanted {
        let mut left = *count;
        while left > 0 {
            let room = max - size;
            let take = left.min(room);
            current.push((*id, title.clone(), take));
            size += take;
            left -= take;
            if size == max {
                batches.push(std::mem::take(&mut current));
                size = 0;
            }
        }
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

/// Checks the model's questions for one request and keeps the good ones,
/// at most `want`.
fn accept(
    drafts: Vec<ExamQuestionDraft>,
    batch: &Batch,
    style: AnswerStyle,
    want: usize,
    seed: u64,
) -> Vec<MockQuestion> {
    let mut kept = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, draft) in drafts.into_iter().enumerate() {
        if kept.len() == want {
            break;
        }
        let prompt = draft.prompt.trim().to_owned();
        let key: String = fold(&prompt);
        if prompt.is_empty() || !seen.insert(key) {
            continue;
        }
        let options: Vec<String> = match style {
            AnswerStyle::TrueFalse => Vec::new(),
            AnswerStyle::Choice { .. } => {
                draft.options.iter().map(|o| o.trim().to_owned()).collect()
            }
        };
        let valid = match style {
            AnswerStyle::TrueFalse => draft.correct < 2,
            AnswerStyle::Choice { options: n } => {
                options.len() == usize::from(n)
                    && options.iter().all(|o| !o.is_empty())
                    && draft.correct < options.len()
            }
        };
        if !valid {
            continue;
        }
        let topic = draft
            .topic
            .checked_sub(1)
            .and_then(|i| batch.topics.get(i))
            .or_else(|| batch.topics.first())
            .copied();
        let mut question = MockQuestion {
            section: batch.section,
            topic,
            prompt,
            options,
            correct: draft.correct as u8,
            explanation: draft.explanation.trim().to_owned(),
        };
        if matches!(style, AnswerStyle::Choice { .. }) {
            let mut shuffled = QuizQuestion {
                prompt: String::new(),
                options: std::mem::take(&mut question.options),
                correct: usize::from(question.correct),
                explanation: String::new(),
                from_board: false,
            };
            shuffled.shuffle_options(seed.wrapping_add(i as u64 * 7919));
            question.options = shuffled.options;
            question.correct = shuffled.correct as u8;
        }
        kept.push(question);
    }
    kept
}

/// Runs every request, a few at a time, and puts the questions in exam
/// order. A request that comes back short is asked once more for the rest.
/// Where a worker leaves one request's questions.
type Slot = Mutex<Option<Result<Vec<MockQuestion>, EngineError>>>;

fn write_questions(
    client: &kg_ai::Client,
    batches: Vec<Batch>,
    format: &ExamFormat,
    seed: u64,
    done: &AtomicU32,
    cancel: &AtomicBool,
) -> Result<Vec<MockQuestion>, EngineError> {
    let slots: Vec<Slot> = batches.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicU32::new(0);
    std::thread::scope(|scope| {
        for _ in 0..PARALLEL_BATCHES.min(batches.len()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed) as usize;
                    let Some(batch) = batches.get(i) else {
                        break;
                    };
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let result = write_batch(client, batch, format.style, seed ^ (i as u64) << 20);
                    if let Ok(questions) = &result {
                        done.fetch_add(questions.len() as u32, Ordering::Relaxed);
                    }
                    if let Ok(mut slot) = slots[i].lock() {
                        *slot = Some(result);
                    }
                }
            });
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(EngineError::Cancelled);
    }
    let mut questions = Vec::new();
    for slot in slots {
        let result = slot
            .into_inner()
            .ok()
            .flatten()
            .unwrap_or(Err(EngineError::Cancelled));
        questions.extend(result?);
    }
    Ok(questions)
}

fn write_batch(
    client: &kg_ai::Client,
    batch: &Batch,
    style: AnswerStyle,
    seed: u64,
) -> Result<Vec<MockQuestion>, EngineError> {
    let want = batch.input.count() as usize;
    let drafts = kg_ai::generate_exam_questions(client, &batch.input)?;
    let mut kept = accept(drafts, batch, style, want, seed);
    if kept.len() < want {
        let missing = (want - kept.len()) as u32;
        let mut retry = Batch {
            section: batch.section,
            input: batch.input.clone(),
            topics: batch.topics.clone(),
        };
        let counts = spread(missing, retry.input.topics.len());
        for ((_, n), count) in retry.input.topics.iter_mut().zip(counts) {
            *n = count;
        }
        let drafts = kg_ai::generate_exam_questions(client, &retry.input)?;
        let more = accept(
            drafts,
            &retry,
            style,
            want - kept.len(),
            seed.wrapping_mul(31),
        );
        let known: BTreeSet<String> = kept.iter().map(|q| fold(&q.prompt)).collect();
        kept.extend(
            more.into_iter()
                .filter(|q| !known.contains(&fold(&q.prompt))),
        );
    }
    if kept.len() < want {
        return Err(EngineError::MockShort);
    }
    kept.truncate(want);
    // Keep each batch's questions grouped by topic, as listed.
    kept.sort_by_key(|q| {
        batch
            .topics
            .iter()
            .position(|t| Some(*t) == q.topic)
            .unwrap_or(usize::MAX)
    });
    Ok(kept)
}

/// The printable exam: questions numbered across the booklet, the essay
/// sheet and the answer card.
fn exam_document(exam: &MockExam, print: &MockPrint, essay: Option<EssaySheet>) -> ExamDocument {
    let mut sections: Vec<ExamPageSection> = Vec::new();
    for (n, question) in exam.questions.iter().enumerate() {
        let subject = exam
            .format
            .sections
            .get(question.section)
            .map(|s| s.subject.clone())
            .unwrap_or_default();
        let page_question = ExamPageQuestion {
            number: n as u32 + 1,
            prompt: question.prompt.clone(),
            options: question.options.clone(),
        };
        match sections.last_mut() {
            Some(last) if last.subject == subject => last.questions.push(page_question),
            _ => sections.push(ExamPageSection {
                subject,
                questions: vec![page_question],
            }),
        }
    }
    let marks = match exam.format.style {
        AnswerStyle::Choice { options } => print
            .choice_marks
            .iter()
            .take(usize::from(options))
            .cloned()
            .collect(),
        AnswerStyle::TrueFalse => print.true_false_marks.to_vec(),
    };
    ExamDocument {
        title: exam.title.clone(),
        subtitle: print.subtitle.clone(),
        lang: print.lang.clone(),
        labels: print.labels.clone(),
        duration: print.duration.clone(),
        instructions: print.instructions.clone(),
        sections,
        marks,
        total: exam.questions.len() as u32,
        essay,
        card: CardLayout::new(mock_code(exam.id)),
    }
}

/// The code printed on an exam's pages and answer card.
pub fn mock_code(id: MockExamId) -> String {
    format!("KG-{}", id.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kg_ai::FormatSectionDraft;

    fn subjects() -> Vec<String> {
        vec![
            "Língua Portuguesa".into(),
            "Direito Constitucional".into(),
            "Informática".into(),
        ]
    }

    fn draft(sections: &[(&str, u32)]) -> FormatDraft {
        FormatDraft {
            found: true,
            minutes: 270,
            style: "choice".into(),
            options: 5,
            wrong_cancels: false,
            essay: true,
            sections: sections
                .iter()
                .map(|(s, n)| FormatSectionDraft {
                    subject: (*s).into(),
                    questions: *n,
                    weight: 1.0,
                })
                .collect(),
        }
    }

    #[test]
    fn estimate_gives_every_subject_questions_and_time() {
        let f = estimate_format(&subjects(), 5, true);
        assert_eq!(f.total_questions(), 30);
        assert_eq!(f.minutes, 150);
        assert_eq!(f.source, FormatSource::Estimate);
        assert_eq!(f.validate(), Ok(()));
    }

    #[test]
    fn format_follows_the_notice_when_its_numbers_are_there() {
        let rules = "A prova terá 15 questões de Língua Portuguesa e 25 de Direito Constitucional, duração de 4h30.";
        let fallback = estimate_format(&subjects(), 4, true);
        let f = format_from_draft(
            &draft(&[("Portugues", 15), ("Direito Constitucional", 25)]),
            &subjects(),
            rules,
            &fallback,
        )
        .unwrap();
        assert_eq!(f.sections[0].questions, 15);
        assert_eq!(f.sections[1].questions, 25);
        assert_eq!(f.sections[2].questions, 0);
        assert_eq!(f.minutes, 270);
        assert_eq!(f.style, AnswerStyle::Choice { options: 5 });
        assert_eq!(f.source, FormatSource::Notice);
    }

    #[test]
    fn invented_numbers_are_refused() {
        let fallback = estimate_format(&subjects(), 4, false);
        let f = format_from_draft(
            &draft(&[("Língua Portuguesa", 17), ("Informática", 23)]),
            &subjects(),
            "O edital não fala do número de questões.",
            &fallback,
        );
        assert_eq!(f, None);
    }

    #[test]
    fn essay_needs_a_written_test_in_the_notice() {
        let fallback = estimate_format(&subjects(), 4, false);
        let f = format_from_draft(
            &draft(&[("Informática", 10)]),
            &subjects(),
            "10 questões",
            &fallback,
        )
        .unwrap();
        assert!(!f.essay);
    }

    #[test]
    fn batches_hold_at_most_ten_questions() {
        let wanted = vec![
            (TopicId(1), "a".to_owned(), 7),
            (TopicId(2), "b".to_owned(), 7),
            (TopicId(3), "c".to_owned(), 13),
        ];
        let batches = batch_topics(&wanted, 10);
        let sizes: Vec<u32> = batches
            .iter()
            .map(|b| b.iter().map(|(_, _, n)| n).sum())
            .collect();
        assert_eq!(sizes, vec![10, 10, 7]);
        assert_eq!(sizes.iter().sum::<u32>(), 27);
    }

    #[test]
    fn accept_drops_bad_and_repeated_questions_and_shuffles() {
        let batch = Batch {
            section: 2,
            input: ExamQuestionsInput::default(),
            topics: vec![TopicId(5), TopicId(6)],
        };
        let q = |prompt: &str, options: usize, correct: usize, topic: usize| ExamQuestionDraft {
            topic,
            prompt: prompt.into(),
            options: (0..options).map(|i| format!("op {i}")).collect(),
            correct,
            explanation: String::new(),
        };
        let kept = accept(
            vec![
                q("Um", 4, 0, 2),
                q("um", 4, 1, 1),
                q("Dois", 3, 0, 1),
                q("Três", 4, 9, 1),
                q("Quatro", 4, 3, 7),
            ],
            &batch,
            AnswerStyle::Choice { options: 4 },
            5,
            42,
        );
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].topic, Some(TopicId(6)));
        assert_eq!(kept[1].topic, Some(TopicId(5)));
        assert_eq!(kept[0].section, 2);
        assert_eq!(kept[0].options[usize::from(kept[0].correct)], "op 0");
        assert_eq!(kept[1].options[usize::from(kept[1].correct)], "op 3");
    }

    #[test]
    fn true_false_questions_keep_no_options() {
        let batch = Batch {
            section: 0,
            input: ExamQuestionsInput::default(),
            topics: vec![TopicId(1)],
        };
        let kept = accept(
            vec![ExamQuestionDraft {
                topic: 1,
                prompt: "A afirmação.".into(),
                options: vec!["Certo".into(), "Errado".into()],
                correct: 1,
                explanation: String::new(),
            }],
            &batch,
            AnswerStyle::TrueFalse,
            1,
            1,
        );
        assert!(kept[0].options.is_empty());
        assert_eq!(kept[0].correct, 1);
    }
}
