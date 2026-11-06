//! Study reminders: every few hours away from the app, a small Know Giver
//! popup lists the subjects furthest behind. The popup is its own short-lived
//! process (`--reminder`), so it appears even while the main window is
//! hidden in the tray.

use std::process::{Child, Command};
use std::time::{Duration, Instant};

use kg_engine::{StudyPlan, TopicStatus};

/// Intervals offered in Settings, in minutes.
pub(crate) const REMINDER_CHOICES: [u32; 8] = [30, 60, 90, 120, 180, 240, 360, 480];
const MAX_SUBJECTS_IN_REMINDER: usize = 3;
const MAX_SUBJECT_NAME_CHARS: usize = 32;

/// The command-line flag that opens the reminder popup.
pub const REMINDER_FLAG: &str = "--reminder";

#[derive(Debug)]
pub(crate) struct ReminderSchedule {
    last_eligible_at: Instant,
    popup: Option<Child>,
}

impl ReminderSchedule {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            last_eligible_at: now,
            popup: None,
        }
    }

    pub(crate) fn is_due(&mut self, now: Instant, eligible: bool, interval: Duration) -> bool {
        if !eligible {
            self.last_eligible_at = now;
            return false;
        }
        if now.duration_since(self.last_eligible_at) < interval {
            return false;
        }
        self.last_eligible_at = now;
        true
    }

    /// Opens the popup for `plan`, unless the last one is still open.
    pub(crate) fn show(&mut self, plan: &StudyPlan) -> std::io::Result<()> {
        if let Some(child) = self.popup.as_mut() {
            if child.try_wait()?.is_none() {
                return Ok(());
            }
            self.popup = None;
        }
        let Some(progress) = Progress::of(&plan.subjects) else {
            return Ok(());
        };
        let child = Command::new(std::env::current_exe()?)
            .arg(REMINDER_FLAG)
            .arg(crate::i18n::current())
            .args(progress.to_args())
            .spawn()?;
        self.popup = Some(child);
        Ok(())
    }
}

/// What the popup shows: the subjects furthest behind and how many more.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Progress {
    pub(crate) subjects: Vec<(String, u32)>,
    pub(crate) more: usize,
}

impl Progress {
    fn of(all_subjects: &[kg_engine::Subject]) -> Option<Self> {
        let mut subjects: Vec<_> = all_subjects
            .iter()
            .filter(|subject| {
                subject
                    .topics
                    .iter()
                    .any(|topic| topic.status != TopicStatus::Done)
            })
            .collect();
        subjects.sort_by(|left, right| left.progress().total_cmp(&right.progress()));
        if subjects.is_empty() {
            return None;
        }
        let more = subjects.len().saturating_sub(MAX_SUBJECTS_IN_REMINDER);
        let subjects = subjects
            .iter()
            .take(MAX_SUBJECTS_IN_REMINDER)
            .map(|subject| {
                let mut name: String = subject.name.chars().take(MAX_SUBJECT_NAME_CHARS).collect();
                if subject.name.chars().count() > MAX_SUBJECT_NAME_CHARS {
                    name.push('…');
                }
                (name, (subject.progress() * 100.0).round() as u32)
            })
            .collect();
        Some(Self { subjects, more })
    }

    /// One argument per subject (`percent:name`), then the count of the rest.
    fn to_args(&self) -> Vec<String> {
        let mut args: Vec<String> = self
            .subjects
            .iter()
            .map(|(name, percent)| format!("{percent}:{name}"))
            .collect();
        args.push(format!("+{}", self.more));
        args
    }

    /// Reads what [`Self::to_args`] wrote; anything else is skipped.
    pub(crate) fn from_args(args: &[String]) -> Self {
        let mut progress = Self {
            subjects: Vec::new(),
            more: 0,
        };
        for arg in args {
            if let Some(count) = arg.strip_prefix('+') {
                progress.more = count.parse().unwrap_or(0);
            } else if let Some((percent, name)) = arg.split_once(':')
                && let Ok(percent) = percent.parse::<u32>()
            {
                progress.subjects.push((name.to_owned(), percent.min(100)));
            }
        }
        progress
    }
}

/// "30 min", "2 h" or "1 h 30 min" in the UI language.
pub(crate) fn interval_label(minutes: u32) -> String {
    let (hours, rest) = (minutes / 60, minutes % 60);
    match (hours, rest) {
        (0, m) => rust_i18n::t!("settings.study_reminders.minutes", count = m).into_owned(),
        (h, 0) => rust_i18n::t!("settings.study_reminders.hours", count = h).into_owned(),
        (h, m) => format!(
            "{} {}",
            rust_i18n::t!("settings.study_reminders.hours", count = h),
            rust_i18n::t!("settings.study_reminders.minutes", count = m)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REMINDER_INTERVAL: Duration = Duration::from_secs(3 * 60 * 60);

    fn subject(name: &str, statuses: &[TopicStatus]) -> kg_engine::Subject {
        kg_engine::Subject {
            id: kg_engine::SubjectId(1),
            name: name.to_owned(),
            topics: statuses
                .iter()
                .enumerate()
                .map(|(index, status)| kg_engine::Topic {
                    id: kg_engine::TopicId(index as i64),
                    title: format!("Topic {index}"),
                    difficulty: kg_engine::Difficulty::default(),
                    status: *status,
                    week: 1,
                    review: Default::default(),
                })
                .collect(),
        }
    }

    #[test]
    fn reminder_waits_three_hours_and_repeats_from_the_last_reminder() {
        let start = Instant::now();
        let mut schedule = ReminderSchedule::new(start);
        assert!(!schedule.is_due(
            start + REMINDER_INTERVAL - Duration::from_secs(1),
            true,
            REMINDER_INTERVAL
        ));
        assert!(schedule.is_due(start + REMINDER_INTERVAL, true, REMINDER_INTERVAL));
        assert!(!schedule.is_due(start + REMINDER_INTERVAL, true, REMINDER_INTERVAL));
        assert!(schedule.is_due(start + REMINDER_INTERVAL * 2, true, REMINDER_INTERVAL));
    }

    #[test]
    fn ineligible_time_does_not_accumulate_toward_a_reminder() {
        let start = Instant::now();
        let mut schedule = ReminderSchedule::new(start);
        assert!(!schedule.is_due(start + REMINDER_INTERVAL * 2, false, REMINDER_INTERVAL));
        assert!(!schedule.is_due(start + REMINDER_INTERVAL * 2, true, REMINDER_INTERVAL));
        assert!(schedule.is_due(start + REMINDER_INTERVAL * 3, true, REMINDER_INTERVAL));
    }

    #[test]
    fn progress_lists_least_complete_subjects_and_omits_finished_ones() {
        let subjects = vec![
            subject("Português", &[TopicStatus::Done]),
            subject("Matemática", &[TopicStatus::NotStarted]),
            subject("História", &[TopicStatus::Done, TopicStatus::NotStarted]),
            subject("Legislação", &[TopicStatus::NotStarted]),
            subject(
                "Direito",
                &[
                    TopicStatus::Done,
                    TopicStatus::Done,
                    TopicStatus::NotStarted,
                    TopicStatus::NotStarted,
                ],
            ),
        ];

        let progress = Progress::of(&subjects).unwrap();
        let names: Vec<_> = progress.subjects.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Matemática", "Legislação", "História"]);
        assert_eq!(progress.subjects[2].1, 50);
        assert_eq!(progress.more, 1);
        assert!(Progress::of(&subjects[..1]).is_none());
    }

    #[test]
    fn progress_survives_the_command_line() {
        let progress = Progress {
            subjects: vec![("Direito: Penal".into(), 40), ("数学".into(), 0)],
            more: 2,
        };
        assert_eq!(Progress::from_args(&progress.to_args()), progress);
    }
}
