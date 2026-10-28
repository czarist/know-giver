//! Study handouts: written lessons per topic, cached in the database and
//! exported as one PDF.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use kg_ai::{HandoutDraft, TopicContext};
use kg_handout::{Document, Exercise, Labels, Section, TopicHandout};
use kg_study::{QuizQuestion, SubjectId, TopicId};
use serde::{Deserialize, Serialize};

use crate::{Engine, EngineError, Event, Notice};

/// Topics written at the same time. More would hit rate limits.
const PARALLEL_TOPICS: usize = 3;
/// Same syllabus-aware excerpt budget used for plan generation.
const EDITAL_CHARS: usize = 150_000;

/// What goes into the handout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoutScope {
    Topic(TopicId),
    Subject(SubjectId),
    Plan,
}

/// Everything the UI decides about the PDF: where it goes and its fixed
/// words, already translated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoutRequest {
    pub scope: HandoutScope,
    pub path: PathBuf,
    pub title: String,
    pub subtitle: String,
    pub generated: String,
    pub lang: String,
    pub labels: Labels,
    /// Write the text again even where a cached version exists.
    pub regenerate: bool,
}

/// A handout export in progress.
#[derive(Debug)]
pub struct HandoutJob {
    pub total: usize,
    done: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
}

impl HandoutJob {
    /// Topics finished so far.
    pub fn done(&self) -> usize {
        self.done.load(Ordering::Relaxed)
    }
}

struct Item {
    topic_id: TopicId,
    context: TopicContext,
    cached: Option<TopicHandout>,
    source: HandoutSource,
}

/// Exact inputs keep old or differently grounded text out of subsequent exports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct HandoutSource {
    exam: String,
    subject: String,
    topic: String,
    reference: String,
    edital: String,
    prompt: String,
}

impl HandoutSource {
    fn new(context: &TopicContext, edital: &str) -> Self {
        Self {
            exam: context.exam.clone(),
            subject: context.subject.clone(),
            topic: context.topic.clone(),
            reference: context.reference.clone(),
            edital: edital.to_owned(),
            prompt: kg_ai::HANDOUT_PROMPT.to_owned(),
        }
    }

    fn cached(&self, json: &str) -> Option<TopicHandout> {
        let cached: CachedHandout = serde_json::from_str(json).ok()?;
        (cached.source == *self).then_some(cached.content)
    }
}

#[derive(Serialize, Deserialize)]
struct CachedHandout {
    source: HandoutSource,
    content: TopicHandout,
}

impl Engine {
    pub fn handout_job(&self) -> Option<&HandoutJob> {
        self.handout_job.as_ref()
    }

    /// Topics of the active plan that already have a written handout.
    pub fn handouts_cached(&self) -> usize {
        self.handouts_cached
    }

    pub(crate) fn export_handout(&mut self, request: HandoutRequest) -> Result<(), EngineError> {
        if self.handout_job.is_some() {
            return Ok(());
        }
        let plan = self.active.as_ref().ok_or(EngineError::NoActivePlan)?;
        let edital = self.handout_edital()?;
        let topic_ids: Vec<TopicId> = plan
            .topics()
            .filter(|(subject, topic)| match request.scope {
                HandoutScope::Topic(id) => topic.id == id,
                HandoutScope::Subject(id) => subject.id == id,
                HandoutScope::Plan => true,
            })
            .map(|(_, topic)| topic.id)
            .collect();
        if topic_ids.is_empty() {
            return Err(kg_handout::HandoutError::Empty.into());
        }

        let mut items = Vec::with_capacity(topic_ids.len());
        for id in topic_ids {
            let (context, _) = self.topic_context(id)?;
            let source = HandoutSource::new(&context, &edital);
            let cached = if request.regenerate {
                None
            } else {
                self.store
                    .handout(id)?
                    .and_then(|json| source.cached(&json))
            };
            items.push(Item {
                topic_id: id,
                context,
                cached,
                source,
            });
        }

        let done = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        self.handout_job = Some(HandoutJob {
            total: items.len(),
            done: Arc::clone(&done),
            cancel: Arc::clone(&cancel),
        });
        let client = self.ai_client();
        let (tx, waker) = self.sender();
        self.spawn(move || {
            let result = build_handout(&client, items, &request, &done, &cancel, &|id, json| {
                let _ = tx.send(Event::HandoutTopic { topic: id, json });
                waker();
            });
            Event::HandoutFinished(result)
        });
        Ok(())
    }

    pub(crate) fn cancel_handout(&mut self) {
        if let Some(job) = &self.handout_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn on_handout_topic(
        &mut self,
        topic: TopicId,
        json: &str,
    ) -> Result<(), EngineError> {
        self.store.save_handout(topic, json, Self::today())?;
        self.refresh_handout_count();
        Ok(())
    }

    pub(crate) fn on_handout_finished(
        &mut self,
        result: Result<PathBuf, EngineError>,
    ) -> Result<(), EngineError> {
        self.handout_job = None;
        match result {
            Ok(path) => {
                self.notices.push(Notice::HandoutSaved(path));
                Ok(())
            }
            Err(EngineError::Cancelled) => Ok(()),
            Err(err) => Err(err),
        }
    }

    pub(crate) fn refresh_handout_count(&mut self) {
        self.handouts_cached = self.valid_handout_count().unwrap_or(0);
    }

    fn valid_handout_count(&self) -> Result<usize, EngineError> {
        let Some(plan) = &self.active else {
            return Ok(0);
        };
        let edital = self.handout_edital()?;
        let mut count = 0;
        for (_, topic) in plan.topics() {
            if let Some(json) = self.store.handout(topic.id)? {
                let (context, _) = self.topic_context(topic.id)?;
                if HandoutSource::new(&context, &edital)
                    .cached(&json)
                    .is_some()
                {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    pub(crate) fn handout_edital(&self) -> Result<String, EngineError> {
        let plan = self.active.as_ref().ok_or(EngineError::NoActivePlan)?;
        let text = self.store.edital_text(plan.id)?;
        Ok(kg_pdf::EditalText::from_raw(&text).for_prompt(EDITAL_CHARS))
    }
}

/// Writes missing topics (a few at a time), then renders and saves the PDF.
/// `on_written` receives each newly written topic as JSON, for caching.
fn build_handout(
    client: &kg_ai::Client,
    items: Vec<Item>,
    request: &HandoutRequest,
    done: &AtomicUsize,
    cancel: &AtomicBool,
    on_written: &(dyn Fn(TopicId, String) + Sync),
) -> Result<PathBuf, EngineError> {
    let slots: Vec<Mutex<Option<TopicHandout>>> = items
        .iter()
        .map(|item| Mutex::new(item.cached.clone()))
        .collect();
    let next = AtomicUsize::new(0);
    let failure: Mutex<Option<EngineError>> = Mutex::new(None);

    std::thread::scope(|scope| {
        for _ in 0..PARALLEL_TOPICS.min(items.len()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    if item.cached.is_none() {
                        match kg_ai::generate_handout(client, &item.context, &item.source.edital) {
                            Ok(draft) => {
                                let topic = topic_from_draft(draft, &item.context, seed(i));
                                let cached = CachedHandout {
                                    source: item.source.clone(),
                                    content: topic.clone(),
                                };
                                if let Ok(json) = serde_json::to_string(&cached) {
                                    on_written(item.topic_id, json);
                                }
                                if let Ok(mut slot) = slots[i].lock() {
                                    *slot = Some(topic);
                                }
                            }
                            Err(err) => {
                                if let Ok(mut failure) = failure.lock() {
                                    failure.get_or_insert(err.into());
                                }
                                cancel.store(true, Ordering::Relaxed);
                                break;
                            }
                        }
                    }
                    done.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });

    if let Some(err) = failure.into_inner().ok().flatten() {
        return Err(err);
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(EngineError::Cancelled);
    }
    let topics: Vec<TopicHandout> = slots
        .into_iter()
        .filter_map(|slot| slot.into_inner().ok().flatten())
        .collect();
    let document = Document {
        title: request.title.clone(),
        subtitle: request.subtitle.clone(),
        generated: request.generated.clone(),
        lang: request.lang.clone(),
        labels: request.labels.clone(),
        topics,
    };
    let pdf = kg_handout::render_pdf(&document)?;
    std::fs::write(&request.path, pdf).map_err(|source| EngineError::Export {
        path: request.path.clone(),
        source,
    })?;
    tracing::info!(path = %request.path.display(), "handout saved");
    Ok(request.path.clone())
}

fn seed(i: usize) -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
        .wrapping_add(i as u64 * 0x9E37_79B9)
}

/// Maps the model's draft into the handout model, shuffling exercise options
/// (models tend to put the right answer first).
fn topic_from_draft(draft: HandoutDraft, context: &TopicContext, seed: u64) -> TopicHandout {
    let exercises = draft
        .exercises
        .into_iter()
        .enumerate()
        .map(|(i, q)| {
            let mut question = QuizQuestion {
                prompt: q.prompt,
                options: q.options,
                correct: q.correct,
                explanation: q.explanation,
                from_board: false,
            };
            if question.correct < question.options.len() {
                question.shuffle_options(seed.wrapping_add(i as u64));
            }
            Exercise {
                prompt: question.prompt,
                options: question.options,
                correct: question.correct,
                explanation: question.explanation,
            }
        })
        .collect();
    TopicHandout {
        topic: context.topic.clone(),
        subject: context.subject.clone(),
        introduction: draft.introduction,
        sections: draft
            .sections
            .into_iter()
            .map(|s| Section {
                heading: s.heading,
                paragraphs: s.paragraphs,
                key_points: s.key_points,
            })
            .collect(),
        examples: draft.examples,
        pitfalls: draft.pitfalls,
        summary: draft.summary,
        exercises,
    }
    .sanitized()
}

#[cfg(test)]
mod tests {
    use kg_ai::{QuestionDraft, SectionDraft};

    use super::*;

    #[test]
    fn cache_requires_identical_sources_and_rejects_legacy_text() {
        let source = HandoutSource::new(&TopicContext::default(), "Português: crase.");
        let content = TopicHandout::default();
        let json = serde_json::to_string(&CachedHandout {
            source: source.clone(),
            content: content.clone(),
        })
        .unwrap();
        assert_eq!(source.cached(&json), Some(content.clone()));
        assert!(
            source
                .cached(&serde_json::to_string(&content).unwrap())
                .is_none()
        );
        assert!(source.cached("broken JSON").is_none());
        for field in ["edital", "reference", "topic", "exam", "subject", "prompt"] {
            let mut changed = source.clone();
            let target = match field {
                "edital" => &mut changed.edital,
                "reference" => &mut changed.reference,
                "topic" => &mut changed.topic,
                "exam" => &mut changed.exam,
                "subject" => &mut changed.subject,
                _ => &mut changed.prompt,
            };
            target.push_str(" changed");
            assert!(
                changed.cached(&json).is_none(),
                "changed {field} must invalidate cache"
            );
        }
    }

    #[test]
    fn draft_maps_to_topic_and_keeps_right_answers() {
        let draft = HandoutDraft {
            introduction: "Intro".into(),
            sections: vec![SectionDraft {
                heading: "Regra".into(),
                paragraphs: vec!["Texto".into()],
                key_points: vec![],
            }],
            exercises: vec![QuestionDraft {
                prompt: "Qual?".into(),
                options: vec!["certa".into(), "b".into(), "c".into(), "d".into()],
                correct: 0,
                explanation: "Porque.".into(),
            }],
            ..HandoutDraft::default()
        };
        let context = TopicContext {
            exam: "TJCE".into(),
            subject: "Português".into(),
            topic: "Crase".into(),
            ..TopicContext::default()
        };
        let topic = topic_from_draft(draft, &context, 42);
        assert_eq!(topic.topic, "Crase");
        assert_eq!(topic.sections.len(), 1);
        let exercise = &topic.exercises[0];
        assert_eq!(exercise.options[exercise.correct], "certa");
    }
}
