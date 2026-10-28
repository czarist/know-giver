//! Prepares a whole plan at once: a written handout and a bank of quiz
//! questions for every topic, then the plan's PDF. Runs in the background;
//! the app stays usable and quizzes start instantly afterwards.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use kg_ai::{AiError, QuestionDraft, TopicContext};
use kg_handout::{Document, Labels, TopicHandout};
use kg_study::TopicId;

use crate::handouts::{HandoutSource, seed, write_pdf, write_topic};
use crate::{Engine, EngineError, Event, Notice};

/// Topics prepared at the same time. Each one makes several requests, so
/// more would run into rate limits.
const PARALLEL_TOPICS: usize = 3;
/// Questions kept per topic; each quiz draws from them.
pub(crate) const QUIZ_BANK_SIZE: u32 = 10;
/// Waits before retrying a request the server turned away.
const RETRY_WAITS: [Duration; 2] = [Duration::from_secs(15), Duration::from_secs(45)];

/// The translated words of the PDF, decided by the UI. The file goes to
/// [`crate::AppPaths::handouts_dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareRequest {
    /// Start of the file name, as in "apostila-<plan>.pdf".
    pub file_prefix: String,
    /// PDF subtitle when the plan has no exam name.
    pub whole_plan: String,
    pub generated: String,
    pub lang: String,
    pub labels: Labels,
}

/// Preparation in progress.
#[derive(Debug)]
pub struct PrepareJob {
    /// Topics in the plan.
    pub total: usize,
    pub started: Instant,
    done: Arc<AtomicUsize>,
    failed: Arc<AtomicUsize>,
    working_on: Arc<Mutex<Vec<String>>>,
    cancel: Arc<AtomicBool>,
}

impl PrepareJob {
    /// Topics finished (successfully or not).
    pub fn done(&self) -> usize {
        self.done.load(Ordering::Relaxed)
    }

    /// Topics where something could not be written.
    pub fn failed(&self) -> usize {
        self.failed.load(Ordering::Relaxed)
    }

    /// Titles of the topics being written right now.
    pub fn working_on(&self) -> Vec<String> {
        self.working_on
            .lock()
            .map(|t| t.clone())
            .unwrap_or_default()
    }

    /// Rough time left, once at least one topic took real work.
    pub fn remaining(&self) -> Option<Duration> {
        let done = self.done();
        if done == 0 || done >= self.total {
            return None;
        }
        let per_topic = self.started.elapsed() / done as u32;
        Some(per_topic * (self.total - done) as u32)
    }

    pub fn is_cancelling(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// What the preparation produced, for the final notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareSummary {
    pub topics: usize,
    pub failed: usize,
    pub pdf: Option<PathBuf>,
}

pub(crate) enum PrepareEvent {
    Handout { topic: TopicId, json: String },
    QuizBank { topic: TopicId, json: String },
    Finished(Result<PrepareSummary, EngineError>),
}

struct Item {
    topic_id: TopicId,
    title: String,
    context: TopicContext,
    source: HandoutSource,
    handout: Option<TopicHandout>,
    has_quiz_bank: bool,
}

impl Engine {
    pub fn prepare_job(&self) -> Option<&PrepareJob> {
        self.prepare_job.as_ref()
    }

    /// Starts preparing every topic of the active plan. Topics that already
    /// have a handout and a question bank are skipped.
    pub(crate) fn prepare_content(&mut self, request: PrepareRequest) -> Result<(), EngineError> {
        if self.prepare_job.is_some() || self.handout_job.is_some() {
            return Ok(());
        }
        let plan = self.active.as_ref().ok_or(EngineError::NoActivePlan)?;
        let file_name = format!("{}-{}.pdf", request.file_prefix, slug(&plan.title));
        let path = self.paths.handouts_dir().join(file_name);
        let document = Document {
            title: plan.title.clone(),
            subtitle: if plan.exam.is_empty() {
                request.whole_plan.clone()
            } else {
                plan.exam.clone()
            },
            generated: request.generated.clone(),
            lang: request.lang.clone(),
            labels: request.labels.clone(),
            topics: Vec::new(),
        };
        let topic_ids: Vec<TopicId> = plan.topics().map(|(_, t)| t.id).collect();
        if topic_ids.is_empty() {
            return Err(kg_handout::HandoutError::Empty.into());
        }
        let edital = self.handout_edital()?;
        let mut items = Vec::with_capacity(topic_ids.len());
        for id in topic_ids {
            let (context, title) = self.topic_context(id)?;
            let source = HandoutSource::new(&context, &edital);
            let handout = self
                .store
                .handout(id)?
                .and_then(|json| source.cached(&json));
            let has_quiz_bank = self.store.quiz_bank(id)?.is_some();
            items.push(Item {
                topic_id: id,
                title,
                context,
                source,
                handout,
                has_quiz_bank,
            });
        }

        let job = PrepareJob {
            total: items.len(),
            started: Instant::now(),
            done: Arc::default(),
            failed: Arc::default(),
            working_on: Arc::default(),
            cancel: Arc::default(),
        };
        let progress = Progress {
            done: Arc::clone(&job.done),
            failed: Arc::clone(&job.failed),
            working_on: Arc::clone(&job.working_on),
            cancel: Arc::clone(&job.cancel),
        };
        self.prepare_job = Some(job);
        tracing::info!(topics = items.len(), "preparing plan content");
        let client = self.ai_client();
        let (tx, waker) = self.sender();
        self.spawn(move || {
            let send = |event: PrepareEvent| {
                let _ = tx.send(Event::Prepare(event));
                waker();
            };
            let result = prepare(&client, items, document, &path, &progress, &send);
            Event::Prepare(PrepareEvent::Finished(result))
        });
        Ok(())
    }

    pub(crate) fn cancel_prepare(&mut self) {
        if let Some(job) = &self.prepare_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn on_prepare_event(&mut self, event: PrepareEvent) -> Result<(), EngineError> {
        match event {
            PrepareEvent::Handout { topic, json } => {
                self.store.save_handout(topic, &json, Self::today())?;
                self.refresh_handout_count();
            }
            PrepareEvent::QuizBank { topic, json } => {
                self.store.save_quiz_bank(topic, &json, Self::today())?;
            }
            PrepareEvent::Finished(result) => {
                self.prepare_job = None;
                match result {
                    Ok(summary) => self.notices.push(Notice::ContentPrepared(summary)),
                    Err(EngineError::Cancelled) => {}
                    Err(err) => return Err(err),
                }
            }
        }
        Ok(())
    }

    /// Prepared questions for a topic, if any.
    pub(crate) fn quiz_bank(&self, topic: TopicId) -> Vec<QuestionDraft> {
        self.store
            .quiz_bank(topic)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }
}

struct Progress {
    done: Arc<AtomicUsize>,
    failed: Arc<AtomicUsize>,
    working_on: Arc<Mutex<Vec<String>>>,
    cancel: Arc<AtomicBool>,
}

impl Progress {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn set_working(&self, title: &str, working: bool) {
        if let Ok(mut list) = self.working_on.lock() {
            if working {
                list.push(title.to_owned());
            } else if let Some(at) = list.iter().position(|t| t == title) {
                list.remove(at);
            }
        }
    }
}

fn prepare(
    client: &kg_ai::Client,
    items: Vec<Item>,
    mut document: Document,
    path: &std::path::Path,
    progress: &Progress,
    send: &(dyn Fn(PrepareEvent) + Sync),
) -> Result<PrepareSummary, EngineError> {
    let slots: Vec<Mutex<Option<TopicHandout>>> = items
        .iter()
        .map(|item| Mutex::new(item.handout.clone()))
        .collect();
    let next = AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for _ in 0..PARALLEL_TOPICS.min(items.len()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    if progress.cancelled() {
                        break;
                    }
                    progress.set_working(&item.title, true);
                    let ok = prepare_topic(client, item, i, &slots[i], progress, send);
                    progress.set_working(&item.title, false);
                    if !ok && !progress.cancelled() {
                        progress.failed.fetch_add(1, Ordering::Relaxed);
                    }
                    progress.done.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });

    if progress.cancelled() {
        return Err(EngineError::Cancelled);
    }
    document.topics = slots
        .into_iter()
        .filter_map(|slot| slot.into_inner().ok().flatten())
        .collect();
    let pdf = if document.topics.is_empty() {
        None
    } else {
        Some(write_pdf(&document, path)?)
    };
    Ok(PrepareSummary {
        topics: items.len(),
        failed: progress.failed.load(Ordering::Relaxed),
        pdf,
    })
}

/// Writes what is missing for one topic. False when something failed.
fn prepare_topic(
    client: &kg_ai::Client,
    item: &Item,
    i: usize,
    slot: &Mutex<Option<TopicHandout>>,
    progress: &Progress,
    send: &(dyn Fn(PrepareEvent) + Sync),
) -> bool {
    let mut ok = true;
    if item.handout.is_none() {
        match with_retry(progress, || {
            write_topic(client, &item.context, &item.source, seed(i))
        }) {
            Ok((topic, json)) => {
                if let Some(json) = json {
                    send(PrepareEvent::Handout {
                        topic: item.topic_id,
                        json,
                    });
                }
                if let Ok(mut slot) = slot.lock() {
                    *slot = Some(topic);
                }
            }
            Err(error) => {
                tracing::warn!(topic = %item.title, %error, "handout not prepared");
                ok = false;
            }
        }
    }
    if !item.has_quiz_bank && !progress.cancelled() {
        let questions = with_retry(progress, || {
            kg_ai::generate_quiz(client, &item.context, QUIZ_BANK_SIZE)
        });
        match questions.map(|q| serde_json::to_string(&q)) {
            Ok(Ok(json)) => send(PrepareEvent::QuizBank {
                topic: item.topic_id,
                json,
            }),
            Ok(Err(error)) => {
                tracing::warn!(topic = %item.title, %error, "quiz bank not serializable");
                ok = false;
            }
            Err(error) => {
                tracing::warn!(topic = %item.title, %error, "quiz bank not prepared");
                ok = false;
            }
        }
    }
    ok
}

/// Retries after a pause when the server is busy or unreachable.
fn with_retry<T>(
    progress: &Progress,
    mut attempt: impl FnMut() -> Result<T, AiError>,
) -> Result<T, AiError> {
    let mut waits = RETRY_WAITS.iter();
    loop {
        match attempt() {
            Err(error @ (AiError::RateLimited | AiError::Connection { .. })) => {
                let Some(wait) = waits.next() else {
                    return Err(error);
                };
                tracing::info!(%error, secs = wait.as_secs(), "server busy; retrying");
                let until = Instant::now() + *wait;
                while Instant::now() < until {
                    if progress.cancelled() {
                        return Err(error);
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            }
            other => return other,
        }
    }
}

/// File-name friendly version of a title: "Língua Portuguesa" → "lingua-portuguesa".
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'Á' | 'À' | 'Â' | 'Ã' => 'a',
            'é' | 'ê' | 'É' | 'Ê' => 'e',
            'í' | 'Í' => 'i',
            'ó' | 'ô' | 'õ' | 'Ó' | 'Ô' | 'Õ' => 'o',
            'ú' | 'ü' | 'Ú' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            c => c.to_ascii_lowercase(),
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 60 {
            break;
        }
    }
    out.trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Língua Portuguesa"), "lingua-portuguesa");
        assert_eq!(slug("Crase: uso (art. 5º)"), "crase-uso-art-5");
    }

    #[test]
    fn remaining_time_follows_the_pace_so_far() {
        let job = PrepareJob {
            total: 4,
            started: Instant::now() - Duration::from_secs(60),
            done: Arc::new(AtomicUsize::new(2)),
            failed: Arc::default(),
            working_on: Arc::default(),
            cancel: Arc::default(),
        };
        let left = job.remaining().unwrap().as_secs();
        assert!((59..=61).contains(&left), "{left}");
        job.done.store(0, Ordering::Relaxed);
        assert!(job.remaining().is_none());
    }
}
