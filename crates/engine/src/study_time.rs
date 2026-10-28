//! Active study time, excluding unfocused windows, idle reading and suspend gaps.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use chrono::{NaiveDate, Timelike};

use crate::{Engine, LessonPhase};

const IDLE_LIMIT: Duration = Duration::from_secs(5 * 60);
const MAX_TICK_GAP: Duration = Duration::from_secs(5);
const SAVE_INTERVAL: Duration = Duration::from_secs(15);

/// The UI reports presence; the engine decides whether elapsed time counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StudyPresence {
    /// Another screen or an unfocused window.
    #[default]
    Inactive,
    /// A study screen is visible, without input in this frame.
    Visible,
    /// The student interacted with a visible study screen.
    Interacting,
}

#[derive(Debug)]
pub(crate) struct StudyClock {
    day: NaiveDate,
    last_tick: Instant,
    last_save: Instant,
    last_interaction: Option<Instant>,
    presence: StudyPresence,
    saved_today: u64,
    fractional: Duration,
    pending: BTreeMap<NaiveDate, u64>,
}

impl StudyClock {
    pub(crate) fn new(day: NaiveDate, now: Instant) -> Self {
        Self {
            day,
            last_tick: now,
            last_save: now,
            last_interaction: None,
            presence: StudyPresence::Inactive,
            saved_today: 0,
            fractional: Duration::ZERO,
            pending: BTreeMap::new(),
        }
    }

    fn advance(&mut self, now: Instant, day: NaiveDate, millis_since_midnight: u64) {
        let elapsed = now.saturating_duration_since(self.last_tick);
        if elapsed <= MAX_TICK_GAP && self.presence != StudyPresence::Inactive {
            let allowed = self.last_interaction.map_or(Duration::ZERO, |last| {
                IDLE_LIMIT.saturating_sub(self.last_tick.saturating_duration_since(last))
            });
            let counted = elapsed.min(allowed) + self.fractional;
            let millis = counted.as_millis() as u64;
            self.fractional = counted - Duration::from_millis(millis);
            if day == self.day {
                *self.pending.entry(day).or_default() += millis;
            } else {
                // Only the short interval since the last frame can cross midnight.
                let after = millis.min(millis_since_midnight);
                *self.pending.entry(self.day).or_default() += millis - after;
                *self.pending.entry(day).or_default() += after;
            }
        }
        self.last_tick = now;
    }
}

impl Engine {
    /// Updates focused study-screen presence once per frame, before [`Self::poll`].
    pub fn set_study_presence(&mut self, presence: StudyPresence) {
        self.tick_study_time();
        let clock = &mut self.study_clock;
        if presence == StudyPresence::Interacting
            || (clock.presence == StudyPresence::Inactive && presence == StudyPresence::Visible)
        {
            clock.last_interaction = Some(Instant::now());
        }
        clock.presence = presence;
        if presence == StudyPresence::Inactive {
            self.flush_study_time();
        }
    }

    /// Seconds actively studied today across plans, including the unsaved interval.
    pub fn today_study_seconds(&self) -> u64 {
        (self.study_clock.saved_today
            + self
                .study_clock
                .pending
                .get(&self.study_clock.day)
                .copied()
                .unwrap_or(0))
            / 1_000
    }

    pub(crate) fn load_study_time(&mut self) {
        self.study_clock.saved_today = self.store.study_millis(self.study_clock.day).unwrap_or(0);
    }

    pub(crate) fn tick_study_time(&mut self) {
        let now = Instant::now();
        let local = chrono::Local::now();
        let day = local.date_naive();
        let midnight_millis = u64::from(local.num_seconds_from_midnight()) * 1_000
            + u64::from(local.timestamp_subsec_millis());
        let clock = &mut self.study_clock;
        clock.advance(now, day, midnight_millis);
        if day != clock.day {
            clock.day = day;
            self.load_study_time();
            self.refresh_activity();
        }
        if self.study_clock.presence != StudyPresence::Inactive
            && self.lesson.as_ref().is_some_and(|lesson| {
                matches!(lesson.phase, LessonPhase::Listening | LessonPhase::Speaking)
            })
        {
            self.study_clock.last_interaction = Some(now);
        }
        if now.saturating_duration_since(self.study_clock.last_save) >= SAVE_INTERVAL {
            self.flush_study_time();
            self.study_clock.last_save = now;
        }
    }

    pub(crate) fn flush_study_time(&mut self) {
        while let Some((&day, &millis)) = self.study_clock.pending.first_key_value() {
            if millis > 0 {
                if let Err(error) = self.store.add_study_millis(day, millis) {
                    tracing::warn!(%error, "could not save study time");
                    break;
                }
                if day == self.study_clock.day {
                    self.study_clock.saved_today += millis;
                }
            }
            self.study_clock.pending.remove(&day);
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.tick_study_time();
        self.flush_study_time();
        self.stop_audio();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock() -> StudyClock {
        let day = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
        StudyClock::new(day, Instant::now())
    }

    #[test]
    fn only_focused_recent_activity_counts_and_suspend_is_skipped() {
        let mut clock = clock();
        let start = clock.last_tick;
        clock.advance(start + Duration::from_secs(1), clock.day, 10_000);
        assert_eq!(clock.pending.values().sum::<u64>(), 0);
        clock.presence = StudyPresence::Visible;
        clock.last_interaction = Some(start);
        clock.advance(start + Duration::from_secs(2), clock.day, 11_000);
        assert_eq!(clock.pending.values().sum::<u64>(), 1_000);
        clock.advance(start + Duration::from_secs(120), clock.day, 129_000);
        assert_eq!(
            clock.pending.values().sum::<u64>(),
            1_000,
            "suspend must not count"
        );
        clock.last_tick = start + IDLE_LIMIT;
        clock.advance(
            start + IDLE_LIMIT + Duration::from_secs(1),
            clock.day,
            400_000,
        );
        assert_eq!(
            clock.pending.values().sum::<u64>(),
            1_000,
            "idle must not count"
        );
    }

    #[test]
    fn splits_midnight_without_losing_subsecond_time() {
        let mut clock = clock();
        let start = clock.last_tick;
        let yesterday = clock.day;
        let today = yesterday.succ_opt().unwrap();
        clock.presence = StudyPresence::Interacting;
        clock.last_interaction = Some(start);
        clock.advance(start + Duration::from_millis(1_500), today, 500);
        assert_eq!(clock.pending[&yesterday], 1_000);
        assert_eq!(clock.pending[&today], 500);
    }

    #[test]
    fn daily_totals_survive_reopening_and_do_not_mix_days() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("study.sqlite");
        let today = clock().day;
        let mut store = kg_study::Store::open(&db).unwrap();
        store.add_study_millis(today, 1_500).unwrap();
        store.add_study_millis(today, 2_000).unwrap();
        store
            .add_study_millis(today.succ_opt().unwrap(), 7_000)
            .unwrap();
        drop(store);
        let store = kg_study::Store::open(&db).unwrap();
        assert_eq!(store.study_millis(today).unwrap(), 3_500);
        assert_eq!(
            store.study_millis(today.succ_opt().unwrap()).unwrap(),
            7_000
        );
    }
}
