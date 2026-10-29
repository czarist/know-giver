//! Study plans as generated from an edital and as stored.

use chrono::NaiveDate;

use crate::StudyError;
use crate::review::{ReviewGrade, ReviewState};

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub i64);
    };
}

id_type!(PlanId);
id_type!(SubjectId);
id_type!(TopicId);

/// Estimated difficulty of a topic, as suggested by the tutor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Difficulty {
    Easy,
    #[default]
    Medium,
    Hard,
}

impl Difficulty {
    pub const ALL: [Self; 3] = [Self::Easy, Self::Medium, Self::Hard];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Easy => "easy",
            Self::Medium => "medium",
            Self::Hard => "hard",
        }
    }

    /// Lenient parser: accepts English and Portuguese names, defaults to medium.
    pub fn parse_lenient(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "easy" | "facil" | "fácil" | "baixa" => Self::Easy,
            "hard" | "dificil" | "difícil" | "alta" => Self::Hard,
            _ => Self::Medium,
        }
    }
}

/// Where the student is with a topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum TopicStatus {
    #[default]
    NotStarted,
    Studying,
    Reviewing,
    Done,
}

impl TopicStatus {
    pub const ALL: [Self; 4] = [
        Self::NotStarted,
        Self::Studying,
        Self::Reviewing,
        Self::Done,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Studying => "studying",
            Self::Reviewing => "reviewing",
            Self::Done => "done",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == value)
    }
}

/// Review interval (days) after which a topic counts as mastered.
const MASTERED_INTERVAL_DAYS: u32 = 21;

/// A single item from the edital's content list.
#[derive(Debug, Clone, PartialEq)]
pub struct Topic {
    pub id: TopicId,
    pub title: String,
    pub difficulty: Difficulty,
    pub status: TopicStatus,
    /// Suggested study week (1-based).
    pub week: u32,
    pub review: ReviewState,
}

impl Topic {
    /// A lesson on this topic was completed: it moves into review and gets
    /// its first review scheduled.
    pub fn complete_lesson(&mut self, today: NaiveDate) {
        if matches!(self.status, TopicStatus::NotStarted | TopicStatus::Studying) {
            self.status = TopicStatus::Reviewing;
        }
        if self.review.due.is_none() {
            self.review = self.review.schedule_first(today);
        }
    }

    /// Applies a review result. Long intervals mean the topic is mastered.
    pub fn apply_review(&mut self, grade: ReviewGrade, today: NaiveDate) {
        self.review = self.review.review(grade, today);
        self.status = if self.review.interval_days >= MASTERED_INTERVAL_DAYS {
            TopicStatus::Done
        } else {
            TopicStatus::Reviewing
        };
    }

    /// The topic's lesson was taken: it is in review or mastered.
    pub fn lesson_taken(&self) -> bool {
        matches!(self.status, TopicStatus::Reviewing | TopicStatus::Done)
    }

    pub fn is_due(&self, today: NaiveDate) -> bool {
        self.review.is_due(today)
    }
}

/// A subject (matéria) and its topics.
#[derive(Debug, Clone, PartialEq)]
pub struct Subject {
    pub id: SubjectId,
    pub name: String,
    pub topics: Vec<Topic>,
}

impl Subject {
    pub fn progress(&self) -> f32 {
        fraction_done(self.topics.iter())
    }
}

fn fraction_done<'a>(topics: impl Iterator<Item = &'a Topic>) -> f32 {
    let (done, total) = topics.fold((0usize, 0usize), |(done, total), t| {
        (done + usize::from(t.status == TopicStatus::Done), total + 1)
    });
    if total == 0 {
        0.0
    } else {
        done as f32 / total as f32
    }
}

/// A stored study plan.
#[derive(Debug, Clone, PartialEq)]
pub struct StudyPlan {
    pub id: PlanId,
    pub title: String,
    pub exam: String,
    pub weeks: u32,
    pub created_on: NaiveDate,
    pub subjects: Vec<Subject>,
}

impl StudyPlan {
    pub fn topics(&self) -> impl Iterator<Item = (&Subject, &Topic)> {
        self.subjects
            .iter()
            .flat_map(|s| s.topics.iter().map(move |t| (s, t)))
    }

    pub fn topic_count(&self) -> usize {
        self.subjects.iter().map(|s| s.topics.len()).sum()
    }

    /// Fraction of topics marked done, in `0.0..=1.0`.
    pub fn progress(&self) -> f32 {
        fraction_done(self.subjects.iter().flat_map(|s| &s.topics))
    }

    pub fn find_topic(&self, id: TopicId) -> Option<(&Subject, &Topic)> {
        self.topics().find(|(_, t)| t.id == id)
    }

    pub fn find_topic_mut(&mut self, id: TopicId) -> Option<&mut Topic> {
        self.subjects
            .iter_mut()
            .flat_map(|s| s.topics.iter_mut())
            .find(|t| t.id == id)
    }

    /// Topics whose review is due on or before `today`, most overdue first.
    pub fn due_topics(&self, today: NaiveDate) -> Vec<(&Subject, &Topic)> {
        let mut due: Vec<_> = self.topics().filter(|(_, t)| t.is_due(today)).collect();
        due.sort_by_key(|(_, t)| t.review.due);
        due
    }

    pub fn count_with_status(&self, status: TopicStatus) -> usize {
        self.topics().filter(|(_, t)| t.status == status).count()
    }

    /// Week numbers that have at least one topic, in order.
    pub fn week_numbers(&self) -> Vec<u32> {
        let mut weeks: Vec<u32> = self.topics().map(|(_, t)| t.week).collect();
        weeks.sort_unstable();
        weeks.dedup();
        weeks
    }
}

/// Short listing of a stored plan (for choosing between editais).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanSummary {
    pub id: PlanId,
    pub title: String,
    pub exam: String,
    pub created_on: NaiveDate,
    pub topic_count: usize,
    pub done_count: usize,
}

/// A topic before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTopic {
    pub title: String,
    pub difficulty: Difficulty,
    pub week: u32,
}

/// A subject before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSubject {
    pub name: String,
    pub topics: Vec<NewTopic>,
}

/// A validated plan ready to be stored: it has subjects, every subject has
/// topics, and weeks are within `1..=weeks`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPlan {
    title: String,
    exam: String,
    weeks: u32,
    subjects: Vec<NewSubject>,
}

impl NewPlan {
    pub fn new(
        title: impl Into<String>,
        exam: impl Into<String>,
        weeks: u32,
        subjects: Vec<NewSubject>,
    ) -> Result<Self, StudyError> {
        let subjects: Vec<NewSubject> = subjects
            .into_iter()
            .map(|mut s| {
                s.name = s.name.trim().to_owned();
                s.topics.retain(|t| !t.title.trim().is_empty());
                s
            })
            .filter(|s| !s.name.is_empty())
            .collect();
        if subjects.is_empty() {
            return Err(StudyError::EmptyPlan);
        }
        if let Some(empty) = subjects.iter().find(|s| s.topics.is_empty()) {
            return Err(StudyError::EmptySubject(empty.name.clone()));
        }
        let weeks = weeks.max(1);
        let subjects = subjects
            .into_iter()
            .map(|mut s| {
                for topic in &mut s.topics {
                    topic.title = topic.title.trim().to_owned();
                    topic.week = topic.week.clamp(1, weeks);
                }
                s
            })
            .collect();
        Ok(Self {
            title: title.into(),
            exam: exam.into(),
            weeks,
            subjects,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn exam(&self) -> &str {
        &self.exam
    }

    pub fn weeks(&self) -> u32 {
        self.weeks
    }

    pub fn subjects(&self) -> &[NewSubject] {
        &self.subjects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_subject(name: &str, topics: &[&str]) -> NewSubject {
        NewSubject {
            name: name.into(),
            topics: topics
                .iter()
                .map(|t| NewTopic {
                    title: (*t).into(),
                    difficulty: Difficulty::Medium,
                    week: 99,
                })
                .collect(),
        }
    }

    fn topic(id: i64, status: TopicStatus) -> Topic {
        Topic {
            id: TopicId(id),
            title: format!("t{id}"),
            difficulty: Difficulty::Medium,
            status,
            week: 1,
            review: ReviewState::default(),
        }
    }

    #[test]
    fn rejects_empty_plan() {
        assert!(matches!(
            NewPlan::new("x", "", 4, vec![]),
            Err(StudyError::EmptyPlan)
        ));
    }

    #[test]
    fn rejects_subject_without_topics() {
        let err = NewPlan::new("x", "", 4, vec![new_subject("Português", &["  "])]).unwrap_err();
        assert!(matches!(err, StudyError::EmptySubject(name) if name == "Português"));
    }

    #[test]
    fn clamps_weeks_into_plan_length() {
        let plan = NewPlan::new("x", "", 4, vec![new_subject("Português", &["Crase"])]).unwrap();
        assert_eq!(plan.subjects()[0].topics[0].week, 4);
    }

    #[test]
    fn progress_counts_done_topics() {
        let plan = StudyPlan {
            id: PlanId(1),
            title: "TRF".into(),
            exam: String::new(),
            weeks: 4,
            created_on: NaiveDate::default(),
            subjects: vec![Subject {
                id: SubjectId(1),
                name: "Português".into(),
                topics: vec![
                    topic(1, TopicStatus::Done),
                    topic(2, TopicStatus::Studying),
                    topic(3, TopicStatus::NotStarted),
                ],
            }],
        };
        assert_eq!(plan.topic_count(), 3);
        assert!((plan.progress() - 1.0 / 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn lesson_moves_topic_into_review() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        let mut t = topic(1, TopicStatus::NotStarted);
        t.complete_lesson(today);
        assert_eq!(t.status, TopicStatus::Reviewing);
        assert_eq!(t.review.due, today.succ_opt());
    }
}
