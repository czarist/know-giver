//! Edital import, plan generation and plan selection.

use std::path::PathBuf;
use std::time::Instant;

use kg_ai::{PlanDraft, PlanPrefs};
use kg_pdf::Edital;
use kg_study::{
    Difficulty, NewPlan, NewSubject, NewTopic, PlanId, QuizStats, TopicId, TopicStatus,
};

use crate::{Engine, EngineError, Event, ImportState, Notice};

const ACTIVE_PLAN_KEY: &str = "active_plan";
/// Edital characters sent to the model (≈ 40k tokens).
const PROMPT_CHARS: usize = 150_000;

impl Engine {
    pub(crate) fn import_edital(&mut self, path: PathBuf) -> Result<(), EngineError> {
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.import = ImportState::Extracting { file };
        let client = self.ai_client();
        self.spawn(move || {
            Event::EditalExtracted(crate::documents::extract_document(
                &client,
                &path,
                &std::sync::atomic::AtomicBool::new(false),
            ))
        });
        Ok(())
    }

    pub(crate) fn on_edital_extracted(&mut self, result: Result<Edital, EngineError>) {
        match result {
            Ok(edital) => self.import = ImportState::Ready(edital),
            Err(err) => {
                self.import = ImportState::Failed;
                self.notices.push(Notice::Error(err.into()));
            }
        }
    }

    pub(crate) fn generate_plan(&mut self, prefs: PlanPrefs) -> Result<(), EngineError> {
        if self.generating_since.is_some() {
            return Ok(());
        }
        let ImportState::Ready(edital) = &self.import else {
            return Err(EngineError::NoEdital);
        };
        let edital = edital.clone();
        let client = self.ai_client();
        self.generating_since = Some(Instant::now());
        self.spawn(move || {
            let excerpt = edital.text.for_prompt(PROMPT_CHARS);
            let result = kg_ai::generate_plan(&client, &excerpt, &prefs);
            Event::PlanGenerated {
                edital,
                prefs,
                result,
            }
        });
        Ok(())
    }

    pub(crate) fn on_plan_generated(
        &mut self,
        edital: Edital,
        prefs: &PlanPrefs,
        result: Result<PlanDraft, kg_ai::AiError>,
    ) -> Result<(), EngineError> {
        self.generating_since = None;
        let draft = result?;
        let plan = new_plan_from_draft(draft, &edital, prefs.weeks)?;
        let stored = self.store.insert_plan(
            &plan,
            &edital.file_name,
            edital.text.as_str(),
            Self::today(),
        )?;
        tracing::info!(
            id = stored.id.0,
            topics = stored.topic_count(),
            "plan created"
        );
        let id = stored.id;
        self.import = ImportState::Empty;
        self.select_plan(id)?;
        self.notices.push(Notice::PlanCreated);
        Ok(())
    }

    pub(crate) fn select_plan(&mut self, id: PlanId) -> Result<(), EngineError> {
        let plan = self.store.load_plan(id)?;
        self.store.set_setting(ACTIVE_PLAN_KEY, &id.0.to_string())?;
        self.active = Some(plan);
        self.end_sessions();
        self.reload_list();
        self.refresh_stats();
        Ok(())
    }

    pub(crate) fn delete_plan(&mut self, id: PlanId) -> Result<(), EngineError> {
        self.store.delete_plan(id)?;
        if self.active.as_ref().is_some_and(|p| p.id == id) {
            self.active = None;
            self.end_sessions();
            self.store.remove_setting(ACTIVE_PLAN_KEY)?;
        }
        self.reload_plans();
        Ok(())
    }

    pub(crate) fn set_topic_status(
        &mut self,
        id: TopicId,
        status: TopicStatus,
    ) -> Result<(), EngineError> {
        self.update_topic(id, |topic| {
            topic.status = status;
            if status == TopicStatus::Reviewing && topic.review.due.is_none() {
                topic.review = topic.review.schedule_first(Self::today());
            }
        })
    }

    /// Reloads the plan list and the active plan from the database.
    pub(crate) fn reload_plans(&mut self) {
        self.reload_list();
        let saved = self
            .store
            .setting(ACTIVE_PLAN_KEY)
            .ok()
            .flatten()
            .and_then(|v| v.parse::<i64>().ok())
            .map(PlanId);
        let id = saved
            .filter(|id| self.plans.iter().any(|p| p.id == *id))
            .or_else(|| self.plans.first().map(|p| p.id));
        self.active = id.and_then(|id| match self.store.load_plan(id) {
            Ok(plan) => Some(plan),
            Err(err) => {
                self.notices.push(Notice::Error(err.into()));
                None
            }
        });
        self.refresh_stats();
    }

    fn reload_list(&mut self) {
        match self.store.list_plans() {
            Ok(plans) => self.plans = plans,
            Err(err) => self.notices.push(Notice::Error(err.into())),
        }
    }

    pub(crate) fn refresh_stats(&mut self) {
        let Some(id) = self.active.as_ref().map(|p| p.id) else {
            self.quiz_stats = QuizStats::default();
            self.lesson_count = 0;
            self.refresh_board();
            return;
        };
        self.quiz_stats = self.store.quiz_stats(id).unwrap_or_default();
        self.lesson_count = self.store.lesson_count(id).unwrap_or_default();
        self.refresh_board();
        self.reload_list();
    }

    fn end_sessions(&mut self) {
        if self.lesson.is_some() {
            self.stop_audio();
            self.lesson = None;
        }
        self.quiz = None;
        self.review = None;
    }
}

fn new_plan_from_draft(
    draft: PlanDraft,
    edital: &Edital,
    weeks: u32,
) -> Result<NewPlan, EngineError> {
    let title = if draft.title.trim().is_empty() {
        edital.file_name.trim_end_matches(".pdf").to_owned()
    } else {
        draft.title.trim().to_owned()
    };
    let subjects = draft
        .subjects
        .into_iter()
        .map(|s| NewSubject {
            name: s.name,
            topics: s
                .topics
                .into_iter()
                .map(|t| NewTopic {
                    title: t.title,
                    difficulty: Difficulty::parse_lenient(&t.difficulty),
                    week: t.week,
                })
                .collect(),
        })
        .collect();
    Ok(NewPlan::new(title, draft.exam.trim(), weeks, subjects)?)
}

#[cfg(test)]
mod tests {
    use kg_ai::{SubjectDraft, TopicDraft};
    use kg_pdf::EditalText;

    use super::*;

    #[test]
    fn draft_becomes_plan_with_clamped_weeks() {
        let draft = PlanDraft {
            title: String::new(),
            exam: "TRF".into(),
            subjects: vec![SubjectDraft {
                name: "Português".into(),
                topics: vec![TopicDraft {
                    title: "Crase".into(),
                    difficulty: "hard".into(),
                    week: 40,
                }],
            }],
        };
        let edital = Edital {
            file_name: "edital-trf.pdf".into(),
            pages: 1,
            text: EditalText::from_raw("x"),
        };
        let plan = new_plan_from_draft(draft, &edital, 8).unwrap();
        assert_eq!(plan.title(), "edital-trf");
        assert_eq!(plan.subjects()[0].topics[0].week, 8);
        assert_eq!(plan.subjects()[0].topics[0].difficulty, Difficulty::Hard);
    }
}
