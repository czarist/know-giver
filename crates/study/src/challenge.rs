//! Study challenges: what each one asks, how far a plan is along, and when
//! it was met. Pure rules over saved records: the store reads the records
//! and keeps the unlocks, the AI never decides them.

use std::collections::HashMap;

use chrono::NaiveDate;

use crate::plan::{PlanId, StudyPlan, Topic, TopicId};

/// A challenge, and the sticker it gives. The key is saved in the database
/// and never depends on the interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ChallengeId {
    /// Finish the plan's first lesson.
    FirstStep,
    /// Answer right, on a new try, a question missed before.
    Comeback,
    /// Study three different topics of the same plan.
    PlanInMotion,
    /// Take the lesson of every topic planned for one week.
    WeekOnTrack,
}

impl ChallengeId {
    pub const ALL: [Self; 4] = [
        Self::FirstStep,
        Self::Comeback,
        Self::PlanInMotion,
        Self::WeekOnTrack,
    ];

    /// Stable key saved in the database. Never rename one.
    pub fn key(self) -> &'static str {
        match self {
            Self::FirstStep => "first_step",
            Self::Comeback => "comeback",
            Self::PlanInMotion => "plan_in_motion",
            Self::WeekOnTrack => "week_on_track",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }
}

/// Topics studied in different ways needed for [`ChallengeId::PlanInMotion`].
pub const PLAN_IN_MOTION_TOPICS: u32 = 3;

/// What the store saved about one plan's study: the rules' input.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlanRecords {
    /// First day a lesson on the topic was completed.
    pub first_lesson: HashMap<TopicId, NaiveDate>,
    /// First day a quiz on the topic was finished.
    pub first_quiz: HashMap<TopicId, NaiveDate>,
    /// First day a review of the topic was graded (recorded since 0.38).
    pub first_review: HashMap<TopicId, NaiveDate>,
    /// Missed questions answered right on a later try: topic and day.
    pub recoveries: Vec<(TopicId, NaiveDate)>,
    /// Topics with questions still in the mistakes notebook, latest first.
    pub open_mistakes: Vec<TopicId>,
}

/// Whether a challenge is met, and since when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completion {
    Pending,
    /// Met. `day` is `None` when the saved history does not tell the day.
    Met {
        day: Option<NaiveDate>,
    },
}

/// Where to go to move a challenge forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextStep {
    Lesson(TopicId),
    Quiz(TopicId),
    /// Go over the questions missed on this topic.
    Mistakes(TopicId),
    None,
}

/// One challenge in one plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChallengeProgress {
    pub id: ChallengeId,
    pub current: u32,
    pub target: u32,
    pub completion: Completion,
    /// The plan week the counter is about ([`ChallengeId::WeekOnTrack`]).
    pub week: Option<u32>,
    pub next: NextStep,
}

impl ChallengeProgress {
    pub fn is_met(&self) -> bool {
        matches!(self.completion, Completion::Met { .. })
    }

    /// Progress in `0.0..=1.0`.
    pub fn fraction(&self) -> f32 {
        if self.target == 0 {
            return 0.0;
        }
        (self.current as f32 / self.target as f32).clamp(0.0, 1.0)
    }
}

/// A sticker won in a plan, as saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeUnlock {
    pub id: i64,
    pub challenge: ChallengeId,
    /// `None` once the plan was deleted; the sticker stays in the collection.
    pub plan: Option<PlanId>,
    /// The plan's title when the sticker was won.
    pub plan_title: String,
    /// The day the challenge was met, or the day it was found met when the
    /// history did not tell (`date_known` false).
    pub unlocked_on: NaiveDate,
    pub date_known: bool,
    /// The unlock notice was already shown.
    pub announced: bool,
}

/// Every challenge of `plan`, in [`ChallengeId::ALL`] order.
pub fn evaluate(plan: &StudyPlan, records: &PlanRecords) -> Vec<ChallengeProgress> {
    let topics = in_study_order(plan);
    ChallengeId::ALL
        .into_iter()
        .map(|id| match id {
            ChallengeId::FirstStep => first_step(&topics, records),
            ChallengeId::Comeback => comeback(&topics, records),
            ChallengeId::PlanInMotion => plan_in_motion(&topics, records),
            ChallengeId::WeekOnTrack => week_on_track(&topics, records),
        })
        .collect()
}

/// Topics by suggested week, keeping the plan's order inside a week.
fn in_study_order(plan: &StudyPlan) -> Vec<&Topic> {
    let mut topics: Vec<&Topic> = plan.topics().map(|(_, t)| t).collect();
    topics.sort_by_key(|t| t.week);
    topics
}

/// A lesson to take: a topic without a lesson on record, preferring one
/// not marked as studied by hand.
fn first_lesson_to_take(topics: &[&Topic], records: &PlanRecords) -> NextStep {
    let open = |t: &&&Topic| !records.first_lesson.contains_key(&t.id);
    topics
        .iter()
        .filter(open)
        .find(|t| !t.lesson_taken())
        .or_else(|| topics.iter().find(open))
        .map_or(NextStep::None, |t| NextStep::Lesson(t.id))
}

fn first_step(topics: &[&Topic], records: &PlanRecords) -> ChallengeProgress {
    let first = records.first_lesson.values().min().copied();
    ChallengeProgress {
        id: ChallengeId::FirstStep,
        current: u32::from(first.is_some()),
        target: 1,
        completion: match first {
            Some(day) => Completion::Met { day: Some(day) },
            None => Completion::Pending,
        },
        week: None,
        next: if first.is_some() {
            NextStep::None
        } else {
            first_lesson_to_take(topics, records)
        },
    }
}

fn comeback(topics: &[&Topic], records: &PlanRecords) -> ChallengeProgress {
    let first = records.recoveries.iter().map(|(_, day)| *day).min();
    let next = if first.is_some() {
        NextStep::None
    } else if let Some(topic) = records.open_mistakes.first() {
        NextStep::Mistakes(*topic)
    } else {
        // No missed question yet: a quiz on a studied topic finds some.
        topics
            .iter()
            .find(|t| t.lesson_taken())
            .or_else(|| topics.first())
            .map_or(NextStep::None, |t| NextStep::Quiz(t.id))
    };
    ChallengeProgress {
        id: ChallengeId::Comeback,
        current: u32::from(first.is_some()),
        target: 1,
        completion: match first {
            Some(day) => Completion::Met { day: Some(day) },
            None => Completion::Pending,
        },
        week: None,
        next,
    }
}

/// When the topic was first studied (a lesson, a quiz or a graded review).
/// `Some(None)`: studied, but before reviews were dated.
fn first_activity(topic: &Topic, records: &PlanRecords) -> Option<Option<NaiveDate>> {
    let dated = [
        records.first_lesson.get(&topic.id),
        records.first_quiz.get(&topic.id),
        records.first_review.get(&topic.id),
    ]
    .into_iter()
    .flatten()
    .min()
    .copied();
    match dated {
        Some(day) => Some(Some(day)),
        // A review graded well before 0.38 left only the repetition count.
        None if topic.review.repetitions > 0 => Some(None),
        None => None,
    }
}

fn plan_in_motion(topics: &[&Topic], records: &PlanRecords) -> ChallengeProgress {
    let studied: Vec<Option<NaiveDate>> = topics
        .iter()
        .filter_map(|t| first_activity(t, records))
        .collect();
    let count = studied.len() as u32;
    let completion = if count >= PLAN_IN_MOTION_TOPICS {
        let mut days: Vec<NaiveDate> = studied.iter().flatten().copied().collect();
        days.sort_unstable();
        // With an undated topic, the day the third was reached is unknown.
        let day = if days.len() == studied.len() {
            days.get(PLAN_IN_MOTION_TOPICS as usize - 1).copied()
        } else {
            None
        };
        Completion::Met { day }
    } else {
        Completion::Pending
    };
    let next = if count >= PLAN_IN_MOTION_TOPICS {
        NextStep::None
    } else {
        // A topic never studied: its lesson, or a quiz when it was only
        // marked as studied by hand.
        let untouched: Vec<&&Topic> = topics
            .iter()
            .filter(|t| first_activity(t, records).is_none())
            .collect();
        match untouched.iter().find(|t| !t.lesson_taken()) {
            Some(t) => NextStep::Lesson(t.id),
            None => untouched
                .first()
                .map_or(NextStep::None, |t| NextStep::Quiz(t.id)),
        }
    };
    ChallengeProgress {
        id: ChallengeId::PlanInMotion,
        current: count.min(PLAN_IN_MOTION_TOPICS),
        target: PLAN_IN_MOTION_TOPICS,
        completion,
        week: None,
        next,
    }
}

/// A week is done when every topic planned for it is in review or done,
/// the same rule as the plan's check marks ([`Topic::lesson_taken`]): a
/// lesson, or a quiz or review that put the topic into review.
fn week_on_track(topics: &[&Topic], records: &PlanRecords) -> ChallengeProgress {
    let mut weeks: Vec<u32> = topics.iter().map(|t| t.week).collect();
    weeks.dedup();
    let mut met: Option<Option<NaiveDate>> = None;
    // The unfinished week closest to done: (done, total, week).
    let mut closest: Option<(u32, u32, u32)> = None;
    for week in weeks {
        let planned: Vec<&&Topic> = topics.iter().filter(|t| t.week == week).collect();
        let total = planned.len() as u32;
        let done = planned.iter().filter(|t| t.lesson_taken()).count() as u32;
        if done == total {
            // The day the last topic got there, when every one is dated
            // (a topic marked by hand has no record).
            let day = planned
                .iter()
                .map(|t| first_activity(t, records).flatten())
                .collect::<Option<Vec<_>>>()
                .and_then(|days| days.into_iter().max());
            met = Some(match (met, day) {
                (Some(Some(known)), Some(day)) => Some(known.min(day)),
                (Some(Some(known)), None) => Some(known),
                (_, day) => day,
            });
            continue;
        }
        let closer = closest.is_none_or(|(d, t, _)| {
            u64::from(done) * u64::from(t) > u64::from(d) * u64::from(total)
        });
        if closer {
            closest = Some((done, total, week));
        }
    }
    if let Some(day) = met {
        // One full week is the goal: shown as 1/1.
        return ChallengeProgress {
            id: ChallengeId::WeekOnTrack,
            current: 1,
            target: 1,
            completion: Completion::Met { day },
            week: None,
            next: NextStep::None,
        };
    }
    let Some((done, total, week)) = closest else {
        return ChallengeProgress {
            id: ChallengeId::WeekOnTrack,
            current: 0,
            target: 1,
            completion: Completion::Pending,
            week: None,
            next: NextStep::None,
        };
    };
    let next = topics
        .iter()
        .find(|t| t.week == week && !t.lesson_taken())
        .map_or(NextStep::None, |t| NextStep::Lesson(t.id));
    ChallengeProgress {
        id: ChallengeId::WeekOnTrack,
        current: done,
        target: total,
        completion: Completion::Pending,
        week: Some(week),
        next,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Difficulty, Subject, SubjectId, TopicStatus};
    use crate::review::ReviewState;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn topic(id: i64, week: u32) -> Topic {
        Topic {
            id: TopicId(id),
            title: format!("Topic {id}"),
            difficulty: Difficulty::Medium,
            status: TopicStatus::NotStarted,
            week,
            review: ReviewState::default(),
        }
    }

    fn plan(topics: Vec<Topic>) -> StudyPlan {
        StudyPlan {
            id: PlanId(1),
            title: "Plan".into(),
            exam: "Exam".into(),
            weeks: 4,
            created_on: day(1),
            subjects: vec![Subject {
                id: SubjectId(1),
                name: "Subject".into(),
                topics,
            }],
        }
    }

    fn get(progress: &[ChallengeProgress], id: ChallengeId) -> ChallengeProgress {
        *progress.iter().find(|p| p.id == id).unwrap()
    }

    #[test]
    fn keys_are_stable_and_round_trip() {
        let keys: Vec<&str> = ChallengeId::ALL.iter().map(|c| c.key()).collect();
        assert_eq!(
            keys,
            ["first_step", "comeback", "plan_in_motion", "week_on_track"]
        );
        for id in ChallengeId::ALL {
            assert_eq!(ChallengeId::from_key(id.key()), Some(id));
        }
        assert_eq!(ChallengeId::from_key("primeiro_passo"), None);
    }

    #[test]
    fn a_new_plan_has_every_challenge_pending_with_a_next_step() {
        let plan = plan(vec![topic(1, 2), topic(2, 1)]);
        let progress = evaluate(&plan, &PlanRecords::default());
        assert!(progress.iter().all(|p| !p.is_met()));
        // The first lesson is the earliest week's topic.
        assert_eq!(
            get(&progress, ChallengeId::FirstStep).next,
            NextStep::Lesson(TopicId(2))
        );
        assert_eq!(
            get(&progress, ChallengeId::Comeback).next,
            NextStep::Quiz(TopicId(2))
        );
    }

    #[test]
    fn first_lesson_meets_first_step_on_its_day() {
        let plan = plan(vec![topic(1, 1), topic(2, 1)]);
        let mut records = PlanRecords::default();
        records.first_lesson.insert(TopicId(2), day(5));
        records.first_lesson.insert(TopicId(1), day(3));
        let first = get(&evaluate(&plan, &records), ChallengeId::FirstStep);
        assert_eq!(first.completion, Completion::Met { day: Some(day(3)) });
        assert_eq!((first.current, first.target), (1, 1));
    }

    #[test]
    fn plan_in_motion_counts_distinct_topics_once() {
        let plan = plan(vec![topic(1, 1), topic(2, 1), topic(3, 2), topic(4, 2)]);
        let mut records = PlanRecords::default();
        // Lesson and quiz on the same topic count once.
        records.first_lesson.insert(TopicId(1), day(2));
        records.first_quiz.insert(TopicId(1), day(3));
        records.first_quiz.insert(TopicId(2), day(4));
        let partial = get(&evaluate(&plan, &records), ChallengeId::PlanInMotion);
        assert_eq!((partial.current, partial.target), (2, 3));
        assert_eq!(partial.next, NextStep::Lesson(TopicId(3)));
        records.first_review.insert(TopicId(4), day(9));
        let done = get(&evaluate(&plan, &records), ChallengeId::PlanInMotion);
        assert_eq!(done.completion, Completion::Met { day: Some(day(9)) });
    }

    #[test]
    fn an_old_undated_review_counts_but_leaves_the_day_unknown() {
        let mut reviewed = topic(3, 1);
        reviewed.review.repetitions = 2;
        let plan = plan(vec![topic(1, 1), topic(2, 1), reviewed]);
        let mut records = PlanRecords::default();
        records.first_lesson.insert(TopicId(1), day(2));
        records.first_quiz.insert(TopicId(2), day(4));
        let done = get(&evaluate(&plan, &records), ChallengeId::PlanInMotion);
        assert_eq!(done.completion, Completion::Met { day: None });
    }

    #[test]
    fn comeback_needs_a_recovered_question_and_points_to_open_mistakes() {
        let plan = plan(vec![topic(1, 1), topic(2, 1)]);
        let mut records = PlanRecords::default();
        records.open_mistakes.push(TopicId(2));
        let pending = get(&evaluate(&plan, &records), ChallengeId::Comeback);
        assert_eq!(pending.next, NextStep::Mistakes(TopicId(2)));
        records.recoveries.push((TopicId(2), day(7)));
        let met = get(&evaluate(&plan, &records), ChallengeId::Comeback);
        assert_eq!(met.completion, Completion::Met { day: Some(day(7)) });
    }

    #[test]
    fn week_on_track_follows_the_lesson_check_marks() {
        let mut a = topic(1, 1);
        let b = topic(2, 1);
        let c = topic(3, 2);
        a.status = TopicStatus::Reviewing;
        let mut records = PlanRecords::default();
        records.first_lesson.insert(TopicId(1), day(2));
        let pending = get(
            &evaluate(&plan(vec![a.clone(), b.clone(), c.clone()]), &records),
            ChallengeId::WeekOnTrack,
        );
        assert_eq!(pending.week, Some(1));
        assert_eq!((pending.current, pending.target), (1, 2));
        assert_eq!(pending.next, NextStep::Lesson(TopicId(2)));

        let mut b = b;
        b.status = TopicStatus::Done;
        records.first_lesson.insert(TopicId(2), day(6));
        let met = get(
            &evaluate(&plan(vec![a, b, c]), &records),
            ChallengeId::WeekOnTrack,
        );
        assert_eq!(met.completion, Completion::Met { day: Some(day(6)) });
    }

    #[test]
    fn a_week_marked_done_by_hand_is_met_without_a_known_day() {
        let mut a = topic(1, 1);
        a.status = TopicStatus::Done;
        let met = get(
            &evaluate(&plan(vec![a, topic(2, 2)]), &PlanRecords::default()),
            ChallengeId::WeekOnTrack,
        );
        assert_eq!(met.completion, Completion::Met { day: None });
    }
}
