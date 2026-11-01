use std::time::{Duration, Instant};

use kg_engine::{StudyPlan, TopicStatus};
use notify_rust::Notification;
use rust_i18n::t;

const REMINDER_INTERVAL: Duration = Duration::from_secs(3 * 60 * 60);
const MAX_SUBJECTS_IN_NOTIFICATION: usize = 3;
const MAX_SUBJECT_NAME_CHARS: usize = 32;

#[derive(Debug)]
pub(crate) struct ReminderSchedule {
    last_eligible_at: Instant,
}

impl ReminderSchedule {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            last_eligible_at: now,
        }
    }

    pub(crate) fn is_due(&mut self, now: Instant, eligible: bool) -> bool {
        if !eligible {
            self.last_eligible_at = now;
            return false;
        }
        if now.duration_since(self.last_eligible_at) < REMINDER_INTERVAL {
            return false;
        }
        self.last_eligible_at = now;
        true
    }
}

pub(crate) fn show_progress(plan: &StudyPlan) -> Result<(), Box<dyn std::error::Error>> {
    let Some(body) = progress_body(&plan.subjects) else {
        return Ok(());
    };
    let title = t!("reminder.title").into_owned();
    Notification::new()
        .appname("Know Giver")
        .summary(&title)
        .body(&body)
        .show()?;
    Ok(())
}

fn progress_body(all_subjects: &[kg_engine::Subject]) -> Option<String> {
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

    let remaining_subjects = subjects.len().saturating_sub(MAX_SUBJECTS_IN_NOTIFICATION);
    let mut lines: Vec<String> = subjects
        .iter()
        .take(MAX_SUBJECTS_IN_NOTIFICATION)
        .map(|subject| {
            let name: String = subject.name.chars().take(MAX_SUBJECT_NAME_CHARS).collect();
            let name = if subject.name.chars().count() > MAX_SUBJECT_NAME_CHARS {
                format!("{name}…")
            } else {
                name
            };
            t!(
                "reminder.subject_progress",
                subject = name,
                percent = (subject.progress() * 100.0).round() as u32
            )
            .into_owned()
        })
        .collect();
    if remaining_subjects > 0 {
        lines.push(t!("reminder.more_subjects", count = remaining_subjects).into_owned());
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(!schedule.is_due(start + REMINDER_INTERVAL - Duration::from_secs(1), true));
        assert!(schedule.is_due(start + REMINDER_INTERVAL, true));
        assert!(!schedule.is_due(start + REMINDER_INTERVAL, true));
        assert!(schedule.is_due(start + REMINDER_INTERVAL * 2, true));
    }

    #[test]
    fn ineligible_time_does_not_accumulate_toward_a_reminder() {
        let start = Instant::now();
        let mut schedule = ReminderSchedule::new(start);
        assert!(!schedule.is_due(start + REMINDER_INTERVAL * 2, false));
        assert!(!schedule.is_due(start + REMINDER_INTERVAL * 2, true));
        assert!(schedule.is_due(start + REMINDER_INTERVAL * 3, true));
    }

    #[test]
    fn progress_body_lists_least_complete_subjects_and_omits_finished_ones() {
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

        let body = progress_body(&subjects).expect("unfinished subjects should be shown");
        assert!(body.contains("Matemática"));
        assert!(body.contains("Legislação"));
        assert!(body.contains("História"));
        assert!(body.contains("0%"));
        assert!(body.contains("50%"));
        assert!(!body.contains("75%"));
        assert!(!body.contains("Direito"));
        assert!(!body.contains("Português"));
        assert_eq!(body.lines().count(), 4);
    }
}
