//! Study challenges and the sticker collection. Progress comes from what
//! the store saved, through the rules in `kg_study::challenge`; a sticker is
//! granted only from those rules, after the study itself was saved. Grants
//! missed when the app closed are found on the next start.

use std::collections::BTreeMap;

use kg_study::{
    ChallengeId, ChallengeProgress, ChallengeUnlock, Completion, Family, Group, NextStep, StudyPlan,
};

use crate::{Engine, EngineError, QuizStep};

/// What the engine knows about challenges right now.
#[derive(Debug, Default)]
pub(crate) struct ChallengeState {
    /// Challenges of the active plan, in [`ChallengeId::ALL`] order.
    progress: Vec<ChallengeProgress>,
    /// Challenges counted across every plan.
    overall: Vec<ChallengeProgress>,
    /// Every sticker won, in every plan, oldest first.
    unlocks: Vec<ChallengeUnlock>,
    /// The check on start is done: from now on, a challenge found met was
    /// met just now, so its day is today even when the records do not say.
    live: bool,
}

impl Engine {
    /// Challenges of the active plan, in a fixed order. Written-answer ones
    /// only when the plan's notice has a written test.
    pub fn challenges(&self) -> Vec<&ChallengeProgress> {
        let writing = self.writing_rules().has_written_test();
        self.challenges
            .progress
            .iter()
            .filter(|c| writing || c.id.group() != Group::Writing)
            .collect()
    }

    /// Challenges counted across every plan (study time, study days).
    pub fn overall_challenges(&self) -> &[ChallengeProgress] {
        &self.challenges.overall
    }

    /// Every sticker won, in every plan, oldest first.
    pub fn sticker_unlocks(&self) -> &[ChallengeUnlock] {
        &self.challenges.unlocks
    }

    /// The challenge to suggest next: the unfinished one of the active plan
    /// closest to done, among those with somewhere to go.
    pub fn next_challenge(&self) -> Option<&ChallengeProgress> {
        self.challenges()
            .into_iter()
            .filter(|c| !c.is_met() && c.reachable && c.next != NextStep::None)
            .fold(None, |best: Option<&ChallengeProgress>, c| match best {
                Some(b) if b.fraction() >= c.fraction() => Some(b),
                _ => Some(c),
            })
    }

    /// True while a lesson, a quiz question or a review is open: unlock
    /// notices wait, so nothing interrupts the student.
    pub fn studying_now(&self) -> bool {
        self.lesson.is_some()
            || self.review.is_some()
            || self
                .quiz
                .as_ref()
                .is_some_and(|q| matches!(q.step, QuizStep::Question { .. }))
    }

    /// Stickers won and not yet announced, one per challenge, marked as
    /// shown so a restart does not show them again. Empty while
    /// [`Engine::studying_now`].
    pub fn take_sticker_notices(&mut self) -> Vec<ChallengeId> {
        if self.studying_now() {
            return Vec::new();
        }
        let pending: Vec<&ChallengeUnlock> = self
            .challenges
            .unlocks
            .iter()
            .filter(|u| !u.announced)
            .collect();
        if pending.is_empty() {
            return Vec::new();
        }
        let ids: Vec<i64> = pending.iter().map(|u| u.id).collect();
        let mut stickers: Vec<ChallengeId> = pending.iter().map(|u| u.challenge).collect();
        stickers.sort_unstable();
        stickers.dedup();
        if let Err(err) = self.store.mark_unlocks_announced(&ids) {
            // Shown anyway; at worst it is shown once more next time.
            tracing::warn!(%err, "could not mark stickers as announced");
        }
        for unlock in &mut self.challenges.unlocks {
            unlock.announced = true;
        }
        stickers
    }

    /// Recomputes the active plan's challenges and grants what is met.
    pub(crate) fn sync_challenges(&mut self) {
        let result = match self.active.take() {
            Some(plan) => {
                let result = self.grant_met(&plan);
                self.active = Some(plan);
                result.map(|progress| self.challenges.progress = progress)
            }
            None => {
                self.challenges.progress.clear();
                Ok(())
            }
        };
        if let Err(err) = result
            .and_then(|()| self.grant_overall())
            .and_then(|()| self.reload_unlocks())
        {
            tracing::warn!(%err, "could not update the challenges");
        }
    }

    /// After study time is saved: the overall challenges only.
    pub(crate) fn sync_overall_challenges(&mut self) {
        if let Err(err) = self.grant_overall().and_then(|()| self.reload_unlocks()) {
            tracing::warn!(%err, "could not update the overall challenges");
        }
    }

    /// Checks every plan, not only the active one: on start, this grants
    /// what was met earlier, also before stickers existed.
    pub(crate) fn sync_all_challenges(&mut self) {
        let ids: Vec<_> = self.plans.iter().map(|p| p.id).collect();
        for id in ids {
            if self.active.as_ref().is_some_and(|p| p.id == id) {
                continue;
            }
            let result = self
                .store
                .load_plan(id)
                .map_err(EngineError::from)
                .and_then(|plan| self.grant_met(&plan).map(|_| ()));
            if let Err(err) = result {
                tracing::warn!(%err, plan = id.0, "could not check the plan's challenges");
            }
        }
        self.sync_challenges();
        self.challenges.live = true;
    }

    /// The day to save for a challenge met on `day` (`None`: the records do
    /// not say) and whether that day is the real one.
    fn grant_day(&self, day: Option<chrono::NaiveDate>) -> (chrono::NaiveDate, bool) {
        match day {
            Some(day) => (day, true),
            // Met just now: today. Found in the history on start: unknown.
            None => (Self::today(), self.challenges.live),
        }
    }

    /// Evaluates `plan`, saves the stickers it newly won and returns its
    /// challenges.
    fn grant_met(&mut self, plan: &StudyPlan) -> Result<Vec<ChallengeProgress>, EngineError> {
        let records = self.store.plan_records(plan.id)?;
        let progress = kg_study::challenge::evaluate(plan, &records);
        for challenge in &progress {
            let Completion::Met { day } = challenge.completion else {
                continue;
            };
            let (day, known) = self.grant_day(day);
            if self
                .store
                .grant_challenge(challenge.id, plan.id, &plan.title, day, known)?
            {
                tracing::info!(
                    challenge = challenge.id.key(),
                    plan = plan.id.0,
                    "sticker won"
                );
            }
        }
        Ok(progress)
    }

    fn grant_overall(&mut self) -> Result<(), EngineError> {
        let mut records = self.store.overall_records()?;
        // The collection counts the other stickers, each on its first day.
        let mut first: BTreeMap<ChallengeId, chrono::NaiveDate> = BTreeMap::new();
        for unlock in self.store.challenge_unlocks()? {
            if unlock.challenge.family() != Family::Stickers {
                let day = first.entry(unlock.challenge).or_insert(unlock.unlocked_on);
                *day = (*day).min(unlock.unlocked_on);
            }
        }
        records.sticker_days = first.into_values().collect();
        let progress = kg_study::challenge::evaluate_overall(&records);
        for challenge in &progress {
            let Completion::Met { day } = challenge.completion else {
                continue;
            };
            let (day, known) = self.grant_day(day);
            if self
                .store
                .grant_overall_challenge(challenge.id, day, known)?
            {
                tracing::info!(challenge = challenge.id.key(), "sticker won");
            }
        }
        self.challenges.overall = progress;
        Ok(())
    }

    fn reload_unlocks(&mut self) -> Result<(), EngineError> {
        self.challenges.unlocks = self.store.challenge_unlocks()?;
        Ok(())
    }
}
