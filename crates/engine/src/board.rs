//! Exam board material: past exams, answer keys and board handouts. Each
//! question or excerpt is checked against the edital; what does not fit is
//! set aside, not used.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use kg_ai::{BoardContext, BoardQuestionDraft, ExcerptDraft};
use kg_study::{
    BoardDocument, BoardExcerpt, BoardQuestion, BoardQuestionId, DocumentId, DocumentKind,
    ExcerptId, NewBoardExcerpt, NewBoardQuestion, PlanId, QuizQuestion, Relevance, TopicId,
    answer_index,
};

use crate::{Engine, EngineError, Event, Notice};

/// Chunks sent to the model at the same time.
const PARALLEL_CHUNKS: usize = 2;
/// Board items added to a topic's prompts.
const REFERENCE_EXCERPTS: usize = 3;
const REFERENCE_QUESTIONS: usize = 3;
/// Reason stored when the student moves an item by hand.
const MANUAL_REASON: &str = "manual";

/// A board upload being processed.
#[derive(Debug)]
pub struct BoardJob {
    pub file: String,
    pub kind: DocumentKind,
    done: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
}

impl BoardJob {
    /// `(chunks done, chunks total)`; total is 0 while the PDF is read.
    pub fn progress(&self) -> (usize, usize) {
        (
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }
}

/// Board material of the active plan, loaded from the database.
#[derive(Debug, Default)]
pub(crate) struct BoardState {
    pub documents: Vec<BoardDocument>,
    pub questions: Vec<BoardQuestion>,
    pub excerpts: Vec<BoardExcerpt>,
    /// Used questions per topic: how much the board asks about it.
    pub emphasis: HashMap<TopicId, usize>,
}

/// What came out of an exam or material upload.
pub(crate) struct BoardImport {
    pub(crate) plan: PlanId,
    pub(crate) kind: DocumentKind,
    pub(crate) file_name: String,
    pub(crate) text: String,
    pub(crate) questions: Vec<NewBoardQuestion>,
    pub(crate) excerpts: Vec<NewBoardExcerpt>,
}

/// An item the student moves in or out of use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoardItem {
    Question(BoardQuestionId),
    Excerpt(ExcerptId),
}

impl Engine {
    pub fn board_documents(&self) -> &[BoardDocument] {
        &self.board.documents
    }

    pub fn board_questions(&self) -> &[BoardQuestion] {
        &self.board.questions
    }

    pub fn board_excerpts(&self) -> &[BoardExcerpt] {
        &self.board.excerpts
    }

    pub fn board_job(&self) -> Option<&BoardJob> {
        self.board_job.as_ref()
    }

    /// Questions from past exams about this topic.
    pub fn board_emphasis(&self, topic: TopicId) -> usize {
        self.board.emphasis.get(&topic).copied().unwrap_or(0)
    }

    pub(crate) fn refresh_board(&mut self) {
        let Some(plan) = self.active.as_ref().map(|p| p.id) else {
            self.board = BoardState::default();
            self.refresh_handout_count();
            return;
        };
        let loaded = (|| -> Result<BoardState, EngineError> {
            let questions = self.store.board_questions(plan)?;
            let mut emphasis = HashMap::new();
            for q in questions.iter().filter(|q| q.relevance.is_used()) {
                if let Some(topic) = q.topic_id {
                    *emphasis.entry(topic).or_insert(0) += 1;
                }
            }
            Ok(BoardState {
                documents: self.store.board_documents(plan)?,
                excerpts: self.store.board_excerpts(plan)?,
                questions,
                emphasis,
            })
        })();
        match loaded {
            Ok(state) => self.board = state,
            Err(err) => self.notices.push(Notice::Error(err)),
        }
        self.refresh_handout_count();
    }

    /// Reads an exam or board material and classifies it against the plan.
    pub(crate) fn import_board(
        &mut self,
        path: PathBuf,
        kind: DocumentKind,
    ) -> Result<(), EngineError> {
        if self.board_job.is_some() {
            return Ok(());
        }
        let plan = self.active.as_ref().ok_or(EngineError::NoActivePlan)?;
        let context = BoardContext {
            exam: if plan.exam.is_empty() {
                plan.title.clone()
            } else {
                plan.exam.clone()
            },
            topics: plan
                .topics()
                .map(|(s, t)| format!("{} — {}", s.name, t.title))
                .collect(),
        };
        let topic_ids: Vec<TopicId> = plan.topics().map(|(_, t)| t.id).collect();
        let plan_id = plan.id;
        let job = self.start_board_job(&path, kind);
        let client = self.ai_client();
        self.spawn(move || {
            let result = read_board(&client, &path, kind, &context, &topic_ids, &job).map(
                |(file_name, text, questions, excerpts)| BoardImport {
                    plan: plan_id,
                    kind,
                    file_name,
                    text,
                    questions,
                    excerpts,
                },
            );
            Event::BoardImported(result)
        });
        Ok(())
    }

    /// Reads an answer key and applies it to an exam already imported.
    pub(crate) fn import_answer_key(
        &mut self,
        path: PathBuf,
        exam: DocumentId,
    ) -> Result<(), EngineError> {
        if self.board_job.is_some() {
            return Ok(());
        }
        let plan = self.active.as_ref().ok_or(EngineError::NoActivePlan)?;
        let exam_name = if plan.exam.is_empty() {
            plan.title.clone()
        } else {
            plan.exam.clone()
        };
        let job = self.start_board_job(&path, DocumentKind::AnswerKey);
        let client = self.ai_client();
        self.spawn(move || {
            let result = (|| {
                let edital = kg_pdf::extract(&path)?;
                let chunks = kg_ai::chunk_text(edital.text.as_str());
                job.total.store(chunks.len(), Ordering::Relaxed);
                let mut answers = Vec::new();
                for chunk in chunks {
                    if job.cancel.load(Ordering::Relaxed) {
                        return Err(EngineError::Cancelled);
                    }
                    for a in kg_ai::extract_answer_key(&client, &exam_name, &chunk)? {
                        if let (Some(number), Some(answer)) = (a.number, a.answer) {
                            answers.push((number, answer));
                        }
                    }
                    job.done.fetch_add(1, Ordering::Relaxed);
                }
                Ok(answers)
            })();
            Event::AnswerKeyRead { exam, result }
        });
        Ok(())
    }

    fn start_board_job(&mut self, path: &std::path::Path, kind: DocumentKind) -> JobHandles {
        let handles = JobHandles::default();
        self.board_job = Some(BoardJob {
            file: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            kind,
            done: Arc::clone(&handles.done),
            total: Arc::clone(&handles.total),
            cancel: Arc::clone(&handles.cancel),
        });
        handles
    }

    pub(crate) fn cancel_board(&mut self) {
        if let Some(job) = &self.board_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn on_board_imported(
        &mut self,
        result: Result<BoardImport, EngineError>,
    ) -> Result<(), EngineError> {
        self.board_job = None;
        let import = match result {
            Ok(import) => import,
            Err(EngineError::Cancelled) => return Ok(()),
            Err(err) => return Err(err),
        };
        // The plan could have been deleted while the file was read.
        if self.active.as_ref().map(|p| p.id) != Some(import.plan) {
            return Ok(());
        }
        self.store.insert_board_document(
            import.plan,
            import.kind,
            &import.file_name,
            &import.text,
            &import.questions,
            &import.excerpts,
            Self::today(),
        )?;
        let used = import
            .questions
            .iter()
            .filter(|q| q.relevance.is_used())
            .count()
            + import
                .excerpts
                .iter()
                .filter(|e| e.relevance.is_used())
                .count();
        let total = import.questions.len() + import.excerpts.len();
        self.refresh_board();
        self.notices.push(Notice::BoardImported {
            kind: import.kind,
            used,
            set_aside: total - used,
        });
        Ok(())
    }

    pub(crate) fn on_answer_key_read(
        &mut self,
        exam: DocumentId,
        result: Result<Vec<(u32, String)>, EngineError>,
    ) -> Result<(), EngineError> {
        self.board_job = None;
        let answers = match result {
            Ok(answers) => answers,
            Err(EngineError::Cancelled) => return Ok(()),
            Err(err) => return Err(err),
        };
        let options: HashMap<u32, &Vec<String>> = self
            .board
            .questions
            .iter()
            .filter(|q| q.document_id == exam)
            .filter_map(|q| q.number.map(|n| (n, &q.options)))
            .collect();
        let resolved: Vec<(u32, usize)> = answers
            .iter()
            .filter_map(|(number, letter)| {
                let opts = options.get(number)?;
                answer_index(letter, opts).map(|i| (*number, i))
            })
            .collect();
        let applied = self.store.apply_answer_key(exam, &resolved)?;
        self.refresh_board();
        self.notices
            .push(Notice::AnswerKeyApplied { count: applied });
        Ok(())
    }

    pub(crate) fn delete_board_document(&mut self, id: DocumentId) -> Result<(), EngineError> {
        self.store.delete_board_document(id)?;
        self.refresh_board();
        Ok(())
    }

    /// Moves an item in or out of use by hand. Using an item needs a topic.
    pub(crate) fn set_board_item(
        &mut self,
        item: BoardItem,
        relevance: Relevance,
        topic: Option<TopicId>,
    ) -> Result<(), EngineError> {
        let relevance = if topic.is_none() {
            Relevance::OutOfScope
        } else {
            relevance
        };
        match item {
            BoardItem::Question(id) => {
                self.store
                    .set_question_relevance(id, relevance, topic, MANUAL_REASON)?;
            }
            BoardItem::Excerpt(id) => {
                self.store
                    .set_excerpt_relevance(id, relevance, topic, MANUAL_REASON)?;
            }
        }
        self.refresh_board();
        Ok(())
    }

    /// Prompt block with the board's material on a topic.
    pub(crate) fn board_reference(&self, topic: TopicId) -> String {
        let excerpts: Vec<String> = self
            .board
            .excerpts
            .iter()
            .filter(|e| e.topic_id == Some(topic) && e.relevance.is_used())
            .take(REFERENCE_EXCERPTS)
            .map(|e| e.summary.clone())
            .collect();
        let questions: Vec<(String, String)> = self
            .usable_board_questions(topic)
            .into_iter()
            .take(REFERENCE_QUESTIONS)
            .map(|q| (q.prompt.clone(), q.options[q.correct].clone()))
            .collect();
        kg_ai::reference_block(&excerpts, &questions)
    }

    /// Past exam questions on a topic that can go into a quiz: in use, with a
    /// known answer.
    pub(crate) fn usable_board_questions(&self, topic: TopicId) -> Vec<QuizQuestion> {
        self.board
            .questions
            .iter()
            .filter(|q| q.topic_id == Some(topic) && q.relevance.is_used())
            .filter_map(|q| {
                let correct = q.correct.filter(|c| *c < q.options.len())?;
                Some(QuizQuestion {
                    prompt: q.prompt.clone(),
                    options: q.options.clone(),
                    correct,
                    explanation: String::new(),
                    from_board: true,
                })
            })
            .collect()
    }
}

#[derive(Default, Clone)]
struct JobHandles {
    done: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
}

type ReadResult = (String, String, Vec<NewBoardQuestion>, Vec<NewBoardExcerpt>);

/// Extracts the PDF and sends its chunks to the model, a few at a time.
fn read_board(
    client: &kg_ai::Client,
    path: &std::path::Path,
    kind: DocumentKind,
    context: &BoardContext,
    topic_ids: &[TopicId],
    job: &JobHandles,
) -> Result<ReadResult, EngineError> {
    let edital = kg_pdf::extract(path)?;
    let chunks = kg_ai::chunk_text(edital.text.as_str());
    job.total.store(chunks.len(), Ordering::Relaxed);

    let next = AtomicUsize::new(0);
    let questions: Mutex<Vec<(usize, BoardQuestionDraft)>> = Mutex::new(Vec::new());
    let excerpts: Mutex<Vec<(usize, ExcerptDraft)>> = Mutex::new(Vec::new());
    let failure: Mutex<Option<EngineError>> = Mutex::new(None);

    std::thread::scope(|scope| {
        for _ in 0..PARALLEL_CHUNKS.min(chunks.len()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(chunk) = chunks.get(i) else { break };
                    if job.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let outcome = match kind {
                        DocumentKind::Exam => {
                            kg_ai::extract_board_questions(client, context, chunk).map(|qs| {
                                if let Ok(mut all) = questions.lock() {
                                    all.extend(qs.into_iter().map(|q| (i, q)));
                                }
                            })
                        }
                        DocumentKind::Material | DocumentKind::AnswerKey => {
                            kg_ai::extract_board_excerpts(client, context, chunk).map(|es| {
                                if let Ok(mut all) = excerpts.lock() {
                                    all.extend(es.into_iter().map(|e| (i, e)));
                                }
                            })
                        }
                    };
                    if let Err(err) = outcome {
                        if let Ok(mut failure) = failure.lock() {
                            failure.get_or_insert(err.into());
                        }
                        job.cancel.store(true, Ordering::Relaxed);
                        break;
                    }
                    job.done.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });

    if let Some(err) = failure.into_inner().ok().flatten() {
        return Err(err);
    }
    if job.cancel.load(Ordering::Relaxed) {
        return Err(EngineError::Cancelled);
    }
    let mut questions = questions.into_inner().unwrap_or_default();
    questions.sort_by_key(|(chunk, q)| (*chunk, q.number));
    let mut excerpts = excerpts.into_inner().unwrap_or_default();
    excerpts.sort_by_key(|(chunk, _)| *chunk);

    let topic_of = |number: Option<u32>| context.topic_index(number).map(|i| topic_ids[i]);
    let questions = dedupe(questions.into_iter().map(|(_, q)| q))
        .into_iter()
        .map(|q| {
            let options = q.options;
            let correct = q.answer.as_deref().and_then(|a| answer_index(a, &options));
            NewBoardQuestion {
                number: q.number,
                prompt: q.prompt.trim().to_owned(),
                options,
                correct,
                topic_id: topic_of(q.topic),
                relevance: Relevance::parse_lenient(&q.relevance),
                reason: q.reason,
            }
            .normalized()
        })
        .filter(NewBoardQuestion::is_valid)
        .collect();
    let excerpts = excerpts
        .into_iter()
        .map(|(_, e)| {
            NewBoardExcerpt {
                summary: e.summary.trim().to_owned(),
                topic_id: topic_of(e.topic),
                relevance: Relevance::parse_lenient(&e.relevance),
                reason: e.reason,
            }
            .normalized()
        })
        .filter(|e| !e.summary.is_empty())
        .collect();
    Ok((
        edital.file_name,
        edital.text.into_string(),
        questions,
        excerpts,
    ))
}

/// Chunks overlap, so a question can come back twice. Keeps the first copy.
fn dedupe(questions: impl Iterator<Item = BoardQuestionDraft>) -> Vec<BoardQuestionDraft> {
    let mut seen = HashSet::new();
    questions
        .filter(|q| {
            let key: String = q
                .prompt
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .take(120)
                .collect();
            seen.insert((q.number, key))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(number: Option<u32>, prompt: &str) -> BoardQuestionDraft {
        BoardQuestionDraft {
            number,
            prompt: prompt.into(),
            options: vec!["Certo".into(), "Errado".into()],
            answer: None,
            topic: Some(1),
            relevance: "relevant".into(),
            reason: String::new(),
        }
    }

    #[test]
    fn dedupe_drops_overlap_copies() {
        let kept = dedupe(
            vec![
                draft(Some(3), "A crase é obrigatória."),
                draft(Some(3), "A  crase é obrigatória!"),
                draft(Some(4), "A crase é obrigatória."),
            ]
            .into_iter(),
        );
        assert_eq!(kept.len(), 2);
    }
}
