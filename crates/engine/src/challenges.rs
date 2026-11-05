//! Study challenges and the sticker collection. Progress comes from what
//! the store saved, through the rules in `kg_study::challenge`; a sticker is
//! granted only from those rules, after the study itself was saved. Grants
//! missed when the app closed are found on the next start.

use kg_study::{ChallengeId, ChallengeProgress, ChallengeUnlock, Completion, StudyPlan};

use crate::{Engine, EngineError, QuizStep};

/// What the engine knows about challenges right now.
#[derive(Debug, Default)]
pub(crate) struct ChallengeState {
    /// Challenges of the active plan, in [`ChallengeId::ALL`] order.
    progress: Vec<ChallengeProgress>,
    /// Every sticker won, in every plan, oldest first.
    unlocks: Vec<ChallengeUnlock>,
}

impl Engine {
    /// Challenges of the active plan, in a fixed order.
    pub fn challenges(&self) -> &[ChallengeProgress] {
        &self.challenges.progress
    }

    /// Every sticker won, in every plan, oldest first.
    pub fn sticker_unlocks(&self) -> &[ChallengeUnlock] {
        &self.challenges.unlocks
    }

    /// The challenge to suggest next: the unfinished one closest to done.
    pub fn next_challenge(&self) -> Option<&ChallengeProgress> {
        self.challenges
            .progress
            .iter()
            .filter(|c| !c.is_met())
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
        if let Err(err) = result.and_then(|()| self.reload_unlocks()) {
            tracing::warn!(%err, "could not update the challenges");
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
    }

    /// Evaluates `plan`, saves the stickers it newly won and returns its
    /// challenges.
    fn grant_met(&mut self, plan: &StudyPlan) -> Result<Vec<ChallengeProgress>, EngineError> {
        let records = self.store.plan_records(plan.id)?;
        let progress = kg_study::challenge::evaluate(plan, &records);
        let today = Self::today();
        for challenge in progress.iter().filter(|c| c.is_met()) {
            let Completion::Met { day } = challenge.completion else {
                continue;
            };
            // History without the day: noted today, and said so on screen.
            let granted = self.store.grant_challenge(
                challenge.id,
                plan.id,
                &plan.title,
                day.unwrap_or(today),
                day.is_some(),
            )?;
            if granted {
                tracing::info!(
                    challenge = challenge.id.key(),
                    plan = plan.id.0,
                    "sticker won"
                );
            }
        }
        Ok(progress)
    }

    fn reload_unlocks(&mut self) -> Result<(), EngineError> {
        self.challenges.unlocks = self.store.challenge_unlocks()?;
        Ok(())
    }
}
