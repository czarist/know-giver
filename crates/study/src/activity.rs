//! Study activity per day: streaks and gentle milestones.

use chrono::{Days, NaiveDate};

/// What the student did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActivityKind {
    Lesson,
    Quiz,
    Review,
}

impl ActivityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lesson => "lesson",
            Self::Quiz => "quiz",
            Self::Review => "review",
        }
    }
}

/// Days in a row with some study, ending today. A streak still counts while
/// today has no activity yet, as long as yesterday had some.
/// `days` must be distinct and sorted from newest to oldest.
pub fn streak(days: &[NaiveDate], today: NaiveDate) -> u32 {
    let mut expected = match days.first() {
        Some(&d) if d == today => today,
        Some(&d) if Some(d) == today.checked_sub_days(Days::new(1)) => d,
        _ => return 0,
    };
    let mut count = 0;
    for &day in days {
        if day != expected {
            break;
        }
        count += 1;
        match expected.checked_sub_days(Days::new(1)) {
            Some(previous) => expected = previous,
            None => break,
        }
    }
    count
}

/// Streak lengths worth a word of encouragement.
pub const STREAK_MILESTONES: [u32; 4] = [3, 7, 14, 30];
/// Plan progress percentages worth a word of encouragement.
pub const PROGRESS_MILESTONES: [u8; 4] = [25, 50, 75, 100];

/// A milestone reached, if `streak` just hit one.
pub fn streak_milestone(streak: u32) -> Option<u32> {
    STREAK_MILESTONES.contains(&streak).then_some(streak)
}

/// The highest progress milestone at or below `progress` (`0.0..=1.0`).
pub fn progress_milestone(progress: f32) -> Option<u8> {
    let percent = (progress * 100.0).round() as u32;
    PROGRESS_MILESTONES
        .iter()
        .rev()
        .find(|&&m| percent >= u32::from(m))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 5, d).unwrap()
    }

    #[test]
    fn streak_counts_consecutive_days() {
        assert_eq!(streak(&[day(10), day(9), day(8), day(6)], day(10)), 3);
        // Today not studied yet: yesterday's streak still stands.
        assert_eq!(streak(&[day(9), day(8)], day(10)), 2);
        // A gap of one day breaks it.
        assert_eq!(streak(&[day(8), day(7)], day(10)), 0);
        assert_eq!(streak(&[], day(10)), 0);
    }

    #[test]
    fn milestones() {
        assert_eq!(streak_milestone(7), Some(7));
        assert_eq!(streak_milestone(8), None);
        assert_eq!(progress_milestone(0.24), None);
        assert_eq!(progress_milestone(0.5), Some(50));
        assert_eq!(progress_milestone(0.99), Some(75));
        assert_eq!(progress_milestone(1.0), Some(100));
    }
}
