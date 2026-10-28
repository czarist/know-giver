//! Daily activity, streaks, milestones and first-run onboarding.

use kg_study::{ActivityKind, progress_milestone, streak, streak_milestone};

use crate::{Engine, EngineError, Notice};

/// Settings key with the milestones already celebrated.
const MILESTONES_KEY: &str = "milestones_seen";
/// Days of activity loaded to compute the streak.
const STREAK_WINDOW: u32 = 400;

/// Something worth a word of encouragement, shown once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Milestone {
    /// Days studied in a row.
    Streak(u32),
    /// Percent of the active plan done.
    Progress(u8),
}

impl Engine {
    /// Days in a row with study, ending today or yesterday.
    pub fn streak(&self) -> u32 {
        self.streak
    }

    /// True once something was studied today.
    pub fn studied_today(&self) -> bool {
        self.studied_today
    }

    /// First run: no plan yet and the welcome steps were never closed.
    pub fn needs_onboarding(&self) -> bool {
        !self.settings.onboarding_done && self.plans.is_empty()
    }

    pub(crate) fn finish_onboarding(&mut self) -> Result<(), EngineError> {
        self.settings.onboarding_done = true;
        self.settings.save(&mut self.store, &self.env_ai)?;
        Ok(())
    }

    /// Records today's study and celebrates a new streak milestone.
    pub(crate) fn record_activity(&mut self, kind: ActivityKind) -> Result<(), EngineError> {
        let today = Self::today();
        let first_today = !self.studied_today;
        self.store.record_activity(kind, today)?;
        self.refresh_activity();
        if first_today && let Some(days) = streak_milestone(self.streak) {
            self.celebrate(Milestone::Streak(days), &format!("streak:{days}"))?;
        }
        Ok(())
    }

    pub(crate) fn refresh_activity(&mut self) {
        let today = Self::today();
        let days = self.store.activity_days(STREAK_WINDOW).unwrap_or_default();
        self.studied_today = days.first() == Some(&today);
        self.streak = streak(&days, today);
    }

    /// Celebrates the plan crossing 25/50/75/100% done.
    pub(crate) fn check_progress_milestone(&mut self) -> Result<(), EngineError> {
        let Some(plan) = self.active.as_ref() else {
            return Ok(());
        };
        if let Some(percent) = progress_milestone(plan.progress()) {
            let key = format!("progress:{}:{percent}", plan.id.0);
            self.celebrate(Milestone::Progress(percent), &key)?;
        }
        Ok(())
    }

    /// Shows a milestone unless it was already shown.
    fn celebrate(&mut self, milestone: Milestone, key: &str) -> Result<(), EngineError> {
        let mut seen: Vec<String> = self
            .store
            .setting(MILESTONES_KEY)?
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        if seen.iter().any(|k| k == key) {
            return Ok(());
        }
        seen.push(key.to_owned());
        let json = serde_json::to_string(&seen).unwrap_or_default();
        self.store.set_setting(MILESTONES_KEY, &json)?;
        self.notices.push(Notice::Milestone(milestone));
        Ok(())
    }
}
