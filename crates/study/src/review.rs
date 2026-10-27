//! Spaced repetition (SM-2).

use chrono::{Days, NaiveDate};

/// How well the student remembered a topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReviewGrade {
    Again,
    Hard,
    Good,
    Easy,
}

impl ReviewGrade {
    pub const ALL: [Self; 4] = [Self::Again, Self::Hard, Self::Good, Self::Easy];

    /// SM-2 quality on the 0..=5 scale.
    fn quality(self) -> f32 {
        match self {
            Self::Again => 1.0,
            Self::Hard => 3.0,
            Self::Good => 4.0,
            Self::Easy => 5.0,
        }
    }

    /// Grade from a quiz score.
    pub fn from_score(correct: u32, total: u32) -> Self {
        if total == 0 {
            return Self::Again;
        }
        let ratio = correct as f32 / total as f32;
        if ratio >= 0.9 {
            Self::Easy
        } else if ratio >= 0.7 {
            Self::Good
        } else if ratio >= 0.5 {
            Self::Hard
        } else {
            Self::Again
        }
    }

    /// Grade from a 0..=5 score given by the correction model.
    pub fn from_level(level: u8) -> Self {
        match level {
            0..=1 => Self::Again,
            2..=3 => Self::Hard,
            4 => Self::Good,
            _ => Self::Easy,
        }
    }
}

const DEFAULT_EASE: f32 = 2.5;
const MIN_EASE: f32 = 1.3;

/// Spaced repetition state of a topic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReviewState {
    pub repetitions: u32,
    pub interval_days: u32,
    pub ease: f32,
    /// `None` until the topic is studied for the first time.
    pub due: Option<NaiveDate>,
    pub last_reviewed: Option<NaiveDate>,
}

impl Default for ReviewState {
    fn default() -> Self {
        Self {
            repetitions: 0,
            interval_days: 0,
            ease: DEFAULT_EASE,
            due: None,
            last_reviewed: None,
        }
    }
}

impl ReviewState {
    pub fn is_due(&self, today: NaiveDate) -> bool {
        self.due.is_some_and(|due| due <= today)
    }

    /// First review, the day after the topic is first studied.
    pub fn schedule_first(self, today: NaiveDate) -> Self {
        Self {
            interval_days: 1,
            due: today.checked_add_days(Days::new(1)),
            last_reviewed: Some(today),
            ..self
        }
    }

    /// Next state after a review, following SM-2.
    pub fn review(self, grade: ReviewGrade, today: NaiveDate) -> Self {
        let q = grade.quality();
        let (repetitions, interval_days) = if q < 3.0 {
            (0, 1)
        } else {
            let reps = self.repetitions + 1;
            let interval = match reps {
                1 => 1,
                2 => 6,
                _ => (self.interval_days.max(1) as f32 * self.ease).round() as u32,
            };
            (reps, interval)
        };
        let ease = (self.ease + 0.1 - (5.0 - q) * (0.08 + (5.0 - q) * 0.02)).max(MIN_EASE);
        Self {
            repetitions,
            interval_days,
            ease,
            due: today.checked_add_days(Days::new(u64::from(interval_days))),
            last_reviewed: Some(today),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, d).unwrap()
    }

    #[test]
    fn good_reviews_grow_the_interval() {
        let mut s = ReviewState::default().schedule_first(day(1));
        s = s.review(ReviewGrade::Good, day(2));
        assert_eq!(s.interval_days, 1);
        s = s.review(ReviewGrade::Good, day(3));
        assert_eq!(s.interval_days, 6);
        s = s.review(ReviewGrade::Good, day(9));
        assert_eq!(s.interval_days, 15);
        assert_eq!(s.due, Some(day(24)));
    }

    #[test]
    fn again_resets_and_lowers_ease() {
        let s = ReviewState {
            repetitions: 4,
            interval_days: 30,
            ..ReviewState::default()
        }
        .review(ReviewGrade::Again, day(1));
        assert_eq!(s.repetitions, 0);
        assert_eq!(s.interval_days, 1);
        assert!(s.ease < DEFAULT_EASE);
    }

    #[test]
    fn ease_never_drops_below_minimum() {
        let mut s = ReviewState::default();
        for _ in 0..20 {
            s = s.review(ReviewGrade::Again, day(1));
        }
        assert!((s.ease - MIN_EASE).abs() < f32::EPSILON);
    }

    #[test]
    fn grade_from_score() {
        assert_eq!(ReviewGrade::from_score(5, 5), ReviewGrade::Easy);
        assert_eq!(ReviewGrade::from_score(4, 5), ReviewGrade::Good);
        assert_eq!(ReviewGrade::from_score(3, 5), ReviewGrade::Hard);
        assert_eq!(ReviewGrade::from_score(1, 5), ReviewGrade::Again);
        assert_eq!(ReviewGrade::from_score(0, 0), ReviewGrade::Again);
    }
}
