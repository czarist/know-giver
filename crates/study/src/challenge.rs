//! Study challenges: what each one asks, how far the student is along, and
//! when it was met. Pure rules over saved records: the store reads the
//! records and keeps the unlocks, the AI never decides them.
//!
//! The catalog is a table: each challenge is a family (what is measured:
//! lessons, quizzes, study hours…) and a target. Most families belong to one
//! plan and can be met again in another; the overall ones count across every
//! plan.

use std::collections::{BTreeSet, HashMap};

use chrono::NaiveDate;

use crate::plan::{PlanId, StudyPlan, Topic, TopicId, TopicStatus};

/// What a family of challenges measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Family {
    /// Lessons completed in the plan.
    Lessons,
    /// Different days with a lesson in the plan.
    LessonDays,
    /// Quizzes finished.
    Quizzes,
    /// Quiz questions answered right, added up.
    RightAnswers,
    /// Quizzes of five questions or more, all right.
    PerfectQuizzes,
    /// Quizzes of five questions or more with 80 % or more right.
    HighQuizzes,
    /// Missed questions answered right on a new try.
    Recoveries,
    /// Five recoveries and no question left in the mistakes notebook.
    CleanNotebook,
    /// Different topics studied (lesson, quiz or review).
    Coverage,
    /// Share of the plan's topics studied, in percent.
    CoverageShare,
    /// Plan weeks with every topic in review or done.
    Weeks,
    /// Share of the plan's weeks complete, in percent.
    PlanComplete,
    /// Topics mastered (long review intervals).
    Mastered,
    /// Subjects with every topic mastered.
    SubjectsMastered,
    /// Share of the plan's topics mastered, in percent.
    NoticeMastered,
    /// Spoken reviews graded.
    Reviews,
    /// Past exams added to the board.
    PastExams,
    /// Handouts read to the last page.
    HandoutsRead,
    /// Highlights in the plan's handouts.
    Highlights,
    /// Handouts made for topics.
    HandoutsMade,
    /// Written answers graded.
    Essays,
    /// Written answers with 70 % or more of the estimated score.
    EssaysHigh,
    /// Written answers with 90 % or more of the estimated score.
    EssayTop,
    /// Share of the plan's subjects with a topic studied, in percent.
    SubjectsStarted,
    /// Mock exams handed in.
    Mocks,
    /// Mock exams with 70 % or more of the points.
    MocksHigh,
    /// Mock exams with 90 % or more of the points.
    MockTop,
    /// Hours of active study, every plan.
    Hours,
    /// Different days of study, every plan.
    StudyDays,
    /// Hours of active study in a single day.
    LongDay,
    /// Study plans made.
    Plans,
    /// Lessons in every plan.
    AllLessons,
    /// Quizzes in every plan.
    AllQuizzes,
    /// Right answers in every plan.
    AllRightAnswers,
    /// Topics mastered in every plan.
    AllMastered,
    /// Different stickers won (not counting these).
    Stickers,
}

impl Family {
    pub const ALL: [Self; 36] = [
        Self::Lessons,
        Self::LessonDays,
        Self::Quizzes,
        Self::RightAnswers,
        Self::PerfectQuizzes,
        Self::HighQuizzes,
        Self::Recoveries,
        Self::CleanNotebook,
        Self::Coverage,
        Self::CoverageShare,
        Self::Weeks,
        Self::PlanComplete,
        Self::Mastered,
        Self::SubjectsMastered,
        Self::NoticeMastered,
        Self::Reviews,
        Self::PastExams,
        Self::HandoutsRead,
        Self::Highlights,
        Self::HandoutsMade,
        Self::Essays,
        Self::EssaysHigh,
        Self::EssayTop,
        Self::SubjectsStarted,
        Self::Mocks,
        Self::MocksHigh,
        Self::MockTop,
        Self::Hours,
        Self::StudyDays,
        Self::LongDay,
        Self::Plans,
        Self::AllLessons,
        Self::AllQuizzes,
        Self::AllRightAnswers,
        Self::AllMastered,
        Self::Stickers,
    ];

    /// Stable key, for translations and art.
    pub fn key(self) -> &'static str {
        match self {
            Self::Lessons => "lessons",
            Self::LessonDays => "lesson_days",
            Self::Quizzes => "quizzes",
            Self::RightAnswers => "right_answers",
            Self::PerfectQuizzes => "perfect_quizzes",
            Self::HighQuizzes => "high_quizzes",
            Self::Recoveries => "recoveries",
            Self::CleanNotebook => "clean_notebook",
            Self::Coverage => "coverage",
            Self::CoverageShare => "coverage_share",
            Self::Weeks => "weeks",
            Self::PlanComplete => "plan_complete",
            Self::Mastered => "mastered",
            Self::SubjectsMastered => "subjects_mastered",
            Self::NoticeMastered => "notice_mastered",
            Self::Reviews => "reviews",
            Self::PastExams => "past_exams",
            Self::HandoutsRead => "handouts_read",
            Self::Highlights => "highlights",
            Self::HandoutsMade => "handouts_made",
            Self::Essays => "essays",
            Self::EssaysHigh => "essays_high",
            Self::EssayTop => "essay_top",
            Self::SubjectsStarted => "subjects_started",
            Self::Mocks => "mocks",
            Self::MocksHigh => "mocks_high",
            Self::MockTop => "mock_top",
            Self::Hours => "hours",
            Self::StudyDays => "study_days",
            Self::LongDay => "long_day",
            Self::Plans => "plans",
            Self::AllLessons => "all_lessons",
            Self::AllQuizzes => "all_quizzes",
            Self::AllRightAnswers => "all_right_answers",
            Self::AllMastered => "all_mastered",
            Self::Stickers => "stickers",
        }
    }

    pub fn scope(self) -> Scope {
        match self {
            Self::Hours
            | Self::StudyDays
            | Self::LongDay
            | Self::Plans
            | Self::AllLessons
            | Self::AllQuizzes
            | Self::AllRightAnswers
            | Self::AllMastered
            | Self::Stickers => Scope::Overall,
            _ => Scope::Plan,
        }
    }

    /// The kind of study it is about, to group it on screen.
    pub fn group(self) -> Group {
        match self {
            Self::Lessons | Self::LessonDays => Group::Lessons,
            Self::Quizzes | Self::RightAnswers | Self::PerfectQuizzes | Self::HighQuizzes => {
                Group::Quizzes
            }
            Self::Recoveries | Self::CleanNotebook => Group::Mistakes,
            Self::Coverage | Self::CoverageShare | Self::SubjectsStarted => Group::Coverage,
            Self::Weeks | Self::PlanComplete => Group::Weeks,
            Self::Mastered | Self::SubjectsMastered | Self::NoticeMastered => Group::Mastery,
            Self::Reviews
            | Self::PastExams
            | Self::HandoutsRead
            | Self::Highlights
            | Self::HandoutsMade => Group::Materials,
            Self::Essays | Self::EssaysHigh | Self::EssayTop => Group::Writing,
            Self::Mocks | Self::MocksHigh | Self::MockTop => Group::Mocks,
            Self::Hours | Self::StudyDays | Self::LongDay => Group::Time,
            Self::Plans
            | Self::AllLessons
            | Self::AllQuizzes
            | Self::AllRightAnswers
            | Self::AllMastered
            | Self::Stickers => Group::Collection,
        }
    }

    /// Targets are a share of the plan (percent), not a count.
    pub fn in_percent(self) -> bool {
        matches!(
            self,
            Self::CoverageShare | Self::PlanComplete | Self::NoticeMastered | Self::SubjectsStarted
        )
    }

    /// The family's challenges, lowest target first.
    pub fn challenges(self) -> impl Iterator<Item = ChallengeId> {
        ChallengeId::all().filter(move |c| c.family() == self)
    }
}

/// Where a challenge is met.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// In one plan; each plan can meet it once.
    Plan,
    /// Once, across every plan.
    Overall,
}

/// The kind of study a challenge is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    Lessons,
    Quizzes,
    Mistakes,
    Coverage,
    Weeks,
    Mastery,
    Materials,
    Writing,
    Mocks,
    Time,
    Collection,
}

impl Group {
    pub const ALL: [Self; 11] = [
        Self::Lessons,
        Self::Quizzes,
        Self::Mistakes,
        Self::Coverage,
        Self::Weeks,
        Self::Mastery,
        Self::Materials,
        Self::Writing,
        Self::Mocks,
        Self::Time,
        Self::Collection,
    ];

    /// Stable key, for translations.
    pub fn key(self) -> &'static str {
        match self {
            Self::Lessons => "lessons",
            Self::Quizzes => "quizzes",
            Self::Mistakes => "mistakes",
            Self::Coverage => "coverage",
            Self::Weeks => "weeks",
            Self::Mastery => "mastery",
            Self::Materials => "materials",
            Self::Writing => "writing",
            Self::Mocks => "mocks",
            Self::Time => "time",
            Self::Collection => "collection",
        }
    }
}

/// One entry of the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Challenge {
    /// Saved in the database; never renamed.
    pub key: &'static str,
    pub family: Family,
    /// A count, hours for time, or percent for [`Family::in_percent`].
    pub target: u32,
}

const fn c(key: &'static str, family: Family, target: u32) -> Challenge {
    Challenge {
        key,
        family,
        target,
    }
}

/// Every challenge. Keys are saved in the database: never rename one, and
/// add new ones at the end of their family.
pub const CATALOG: [Challenge; 164] = [
    c("first_step", Family::Lessons, 1),
    c("lessons_3", Family::Lessons, 3),
    c("lessons_5", Family::Lessons, 5),
    c("lessons_10", Family::Lessons, 10),
    c("lessons_15", Family::Lessons, 15),
    c("lessons_20", Family::Lessons, 20),
    c("lessons_30", Family::Lessons, 30),
    c("lessons_40", Family::Lessons, 40),
    c("lessons_50", Family::Lessons, 50),
    c("lessons_75", Family::Lessons, 75),
    c("lessons_100", Family::Lessons, 100),
    c("lesson_days_3", Family::LessonDays, 3),
    c("lesson_days_7", Family::LessonDays, 7),
    c("lesson_days_14", Family::LessonDays, 14),
    c("lesson_days_30", Family::LessonDays, 30),
    c("lesson_days_60", Family::LessonDays, 60),
    c("lesson_days_100", Family::LessonDays, 100),
    c("first_quiz", Family::Quizzes, 1),
    c("quizzes_3", Family::Quizzes, 3),
    c("quizzes_5", Family::Quizzes, 5),
    c("quizzes_10", Family::Quizzes, 10),
    c("quizzes_20", Family::Quizzes, 20),
    c("quizzes_30", Family::Quizzes, 30),
    c("quizzes_50", Family::Quizzes, 50),
    c("quizzes_75", Family::Quizzes, 75),
    c("quizzes_100", Family::Quizzes, 100),
    c("right_answers_10", Family::RightAnswers, 10),
    c("right_answers_25", Family::RightAnswers, 25),
    c("right_answers_50", Family::RightAnswers, 50),
    c("right_answers_100", Family::RightAnswers, 100),
    c("right_answers_250", Family::RightAnswers, 250),
    c("right_answers_500", Family::RightAnswers, 500),
    c("right_answers_1000", Family::RightAnswers, 1000),
    c("perfect_quiz", Family::PerfectQuizzes, 1),
    c("perfect_quizzes_3", Family::PerfectQuizzes, 3),
    c("perfect_quizzes_5", Family::PerfectQuizzes, 5),
    c("perfect_quizzes_10", Family::PerfectQuizzes, 10),
    c("perfect_quizzes_25", Family::PerfectQuizzes, 25),
    c("high_quizzes_1", Family::HighQuizzes, 1),
    c("high_quizzes_5", Family::HighQuizzes, 5),
    c("high_quizzes_10", Family::HighQuizzes, 10),
    c("high_quizzes_25", Family::HighQuizzes, 25),
    c("comeback", Family::Recoveries, 1),
    c("recoveries_5", Family::Recoveries, 5),
    c("comeback_10", Family::Recoveries, 10),
    c("recoveries_25", Family::Recoveries, 25),
    c("recoveries_50", Family::Recoveries, 50),
    c("recoveries_100", Family::Recoveries, 100),
    c("clean_notebook", Family::CleanNotebook, 5),
    c("plan_in_motion", Family::Coverage, 3),
    c("coverage_5", Family::Coverage, 5),
    c("coverage_10", Family::Coverage, 10),
    c("coverage_20", Family::Coverage, 20),
    c("coverage_30", Family::Coverage, 30),
    c("coverage_50", Family::Coverage, 50),
    c("coverage_share_25", Family::CoverageShare, 25),
    c("half_notice", Family::CoverageShare, 50),
    c("coverage_share_75", Family::CoverageShare, 75),
    c("whole_notice", Family::CoverageShare, 100),
    c("week_on_track", Family::Weeks, 1),
    c("weeks_2", Family::Weeks, 2),
    c("four_weeks", Family::Weeks, 4),
    c("weeks_8", Family::Weeks, 8),
    c("weeks_12", Family::Weeks, 12),
    c("plan_complete", Family::PlanComplete, 100),
    c("first_mastery", Family::Mastered, 1),
    c("mastered_3", Family::Mastered, 3),
    c("mastered_5", Family::Mastered, 5),
    c("mastery_10", Family::Mastered, 10),
    c("mastered_20", Family::Mastered, 20),
    c("mastered_30", Family::Mastered, 30),
    c("mastered_50", Family::Mastered, 50),
    c("subject_mastered", Family::SubjectsMastered, 1),
    c("subjects_mastered_2", Family::SubjectsMastered, 2),
    c("subjects_mastered_3", Family::SubjectsMastered, 3),
    c("subjects_mastered_5", Family::SubjectsMastered, 5),
    c("notice_mastered_100", Family::NoticeMastered, 100),
    c("first_review", Family::Reviews, 1),
    c("reviews_5", Family::Reviews, 5),
    c("reviews_10", Family::Reviews, 10),
    c("reviews_25", Family::Reviews, 25),
    c("reviews_50", Family::Reviews, 50),
    c("reviews_100", Family::Reviews, 100),
    c("past_exam", Family::PastExams, 1),
    c("past_exams_3", Family::PastExams, 3),
    c("past_exams_5", Family::PastExams, 5),
    c("handout_read", Family::HandoutsRead, 1),
    c("handouts_read_3", Family::HandoutsRead, 3),
    c("handouts_read_5", Family::HandoutsRead, 5),
    c("handouts_read_10", Family::HandoutsRead, 10),
    c("highlights_1", Family::Highlights, 1),
    c("highlights_10", Family::Highlights, 10),
    c("highlights_25", Family::Highlights, 25),
    c("highlights_50", Family::Highlights, 50),
    c("highlights_100", Family::Highlights, 100),
    c("handouts_made_1", Family::HandoutsMade, 1),
    c("handouts_made_5", Family::HandoutsMade, 5),
    c("handouts_made_10", Family::HandoutsMade, 10),
    c("handouts_made_25", Family::HandoutsMade, 25),
    c("first_essay", Family::Essays, 1),
    c("essays_3", Family::Essays, 3),
    c("essays_5", Family::Essays, 5),
    c("essays_10", Family::Essays, 10),
    c("essays_20", Family::Essays, 20),
    c("essay_high", Family::EssaysHigh, 1),
    c("essays_high_3", Family::EssaysHigh, 3),
    c("essays_high_5", Family::EssaysHigh, 5),
    c("essay_top_1", Family::EssayTop, 1),
    c("subjects_started_100", Family::SubjectsStarted, 100),
    c("first_mock", Family::Mocks, 1),
    c("mocks_3", Family::Mocks, 3),
    c("mocks_5", Family::Mocks, 5),
    c("mocks_10", Family::Mocks, 10),
    c("mocks_20", Family::Mocks, 20),
    c("mocks_high_1", Family::MocksHigh, 1),
    c("mocks_high_3", Family::MocksHigh, 3),
    c("mocks_high_5", Family::MocksHigh, 5),
    c("mocks_high_10", Family::MocksHigh, 10),
    c("mock_top_1", Family::MockTop, 1),
    c("hours_1", Family::Hours, 1),
    c("hours_5", Family::Hours, 5),
    c("hours_10", Family::Hours, 10),
    c("hours_25", Family::Hours, 25),
    c("hours_50", Family::Hours, 50),
    c("hours_75", Family::Hours, 75),
    c("hours_100", Family::Hours, 100),
    c("hours_150", Family::Hours, 150),
    c("hours_200", Family::Hours, 200),
    c("hours_300", Family::Hours, 300),
    c("hours_500", Family::Hours, 500),
    c("study_days_3", Family::StudyDays, 3),
    c("days_7", Family::StudyDays, 7),
    c("study_days_14", Family::StudyDays, 14),
    c("days_30", Family::StudyDays, 30),
    c("study_days_60", Family::StudyDays, 60),
    c("study_days_100", Family::StudyDays, 100),
    c("study_days_180", Family::StudyDays, 180),
    c("study_days_365", Family::StudyDays, 365),
    c("long_day_1", Family::LongDay, 1),
    c("long_day_2", Family::LongDay, 2),
    c("long_day_3", Family::LongDay, 3),
    c("long_day_4", Family::LongDay, 4),
    c("plans_1", Family::Plans, 1),
    c("plans_2", Family::Plans, 2),
    c("plans_3", Family::Plans, 3),
    c("all_lessons_10", Family::AllLessons, 10),
    c("all_lessons_50", Family::AllLessons, 50),
    c("all_lessons_100", Family::AllLessons, 100),
    c("all_lessons_250", Family::AllLessons, 250),
    c("all_lessons_500", Family::AllLessons, 500),
    c("all_quizzes_10", Family::AllQuizzes, 10),
    c("all_quizzes_50", Family::AllQuizzes, 50),
    c("all_quizzes_100", Family::AllQuizzes, 100),
    c("all_quizzes_250", Family::AllQuizzes, 250),
    c("all_right_answers_1000", Family::AllRightAnswers, 1000),
    c("all_right_answers_2500", Family::AllRightAnswers, 2500),
    c("all_right_answers_5000", Family::AllRightAnswers, 5000),
    c("all_mastered_25", Family::AllMastered, 25),
    c("all_mastered_50", Family::AllMastered, 50),
    c("all_mastered_100", Family::AllMastered, 100),
    c("stickers_10", Family::Stickers, 10),
    c("stickers_25", Family::Stickers, 25),
    c("stickers_50", Family::Stickers, 50),
    c("stickers_100", Family::Stickers, 100),
];

/// A challenge, and the sticker it gives: an entry of [`CATALOG`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChallengeId(u16);

impl ChallengeId {
    /// Every challenge, in catalog order.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..CATALOG.len() as u16).map(Self)
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::all().find(|c| c.key() == key)
    }

    fn entry(self) -> &'static Challenge {
        // Ids are only made from catalog positions.
        &CATALOG[usize::from(self.0) % CATALOG.len()]
    }

    /// Stable key saved in the database.
    pub fn key(self) -> &'static str {
        self.entry().key
    }

    pub fn family(self) -> Family {
        self.entry().family
    }

    pub fn target(self) -> u32 {
        self.entry().target
    }

    pub fn scope(self) -> Scope {
        self.family().scope()
    }

    pub fn group(self) -> Group {
        self.family().group()
    }

    /// Its level in the family, from 1.
    pub fn level(self) -> u32 {
        self.family()
            .challenges()
            .position(|c| c == self)
            .map_or(1, |i| i as u32 + 1)
    }
}

/// Mistakes answered right before [`Family::CleanNotebook`] counts.
pub const CLEAN_NOTEBOOK_RECOVERIES: u32 = 5;
/// Questions in a quiz for it to count as perfect or high.
pub const QUIZ_MIN_QUESTIONS: u32 = 5;
/// Share right for [`Family::HighQuizzes`].
pub const HIGH_QUIZ_SHARE: f64 = 0.8;
/// Share of the score for [`Family::EssaysHigh`].
pub const ESSAY_HIGH_SHARE: f64 = 0.7;
/// Share of the score for [`Family::EssayTop`].
pub const ESSAY_TOP_SHARE: f64 = 0.9;
/// Share of the points for [`Family::MocksHigh`].
pub const MOCK_HIGH_SHARE: f64 = 0.7;
/// Share of the points for [`Family::MockTop`].
pub const MOCK_TOP_SHARE: f64 = 0.9;
const HOUR_MS: u64 = 3_600_000;

/// A finished quiz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuizRecord {
    pub day: NaiveDate,
    pub correct: u32,
    pub total: u32,
}

/// A graded written answer: its estimated score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EssayRecord {
    pub day: NaiveDate,
    pub total: f64,
    pub max: f64,
}

/// What the store saved about one plan's study: the rules' input.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlanRecords {
    /// First day a lesson on the topic was completed.
    pub first_lesson: HashMap<TopicId, NaiveDate>,
    /// First day a quiz on the topic was finished.
    pub first_quiz: HashMap<TopicId, NaiveDate>,
    /// First day a review of the topic was graded (recorded since 0.38).
    pub first_review: HashMap<TopicId, NaiveDate>,
    /// Every completed lesson's day.
    pub lesson_days: Vec<NaiveDate>,
    pub quizzes: Vec<QuizRecord>,
    /// Every graded review's day (one per topic and day).
    pub review_days: Vec<NaiveDate>,
    /// Missed questions answered right on a later try: topic and day.
    pub recoveries: Vec<(TopicId, NaiveDate)>,
    /// Topics with questions still in the mistakes notebook, latest first.
    pub open_mistakes: Vec<TopicId>,
    /// Days past exams were added to the board.
    pub exam_days: Vec<NaiveDate>,
    /// Handouts read to the last page (the day is not kept).
    pub readings_finished: u32,
    /// Days highlights were made in the plan's handouts.
    pub highlight_days: Vec<NaiveDate>,
    /// Days handouts were made for the plan's topics.
    pub handout_days: Vec<NaiveDate>,
    pub essays: Vec<EssayRecord>,
    /// Mock exams handed in: the day and the share of the points.
    pub mocks: Vec<(NaiveDate, f64)>,
}

/// Study across every plan: the input of the overall challenges.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverallRecords {
    /// Active study time per day, in milliseconds.
    pub study_millis: Vec<(NaiveDate, u64)>,
    /// Days with a lesson, quiz or review.
    pub activity_days: Vec<NaiveDate>,
    /// The day each existing plan was made.
    pub plan_days: Vec<NaiveDate>,
    /// Every lesson's day, in every plan.
    pub lesson_days: Vec<NaiveDate>,
    /// Every quiz: its day and right answers.
    pub quizzes: Vec<(NaiveDate, u32)>,
    /// Topics mastered in every plan (the day is not kept).
    pub mastered: u32,
    /// The first day each sticker was won (not the sticker challenges).
    pub sticker_days: Vec<NaiveDate>,
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
    Review(TopicId),
    /// The exam board, to add a past exam.
    Board,
    /// The handouts.
    Reader,
    /// Written answers.
    Writing,
    /// Mock exams.
    Mock,
    None,
}

/// One challenge, in one plan or overall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChallengeProgress {
    pub id: ChallengeId,
    /// How far along, in the unit of the target (topics for percent ones).
    pub current: u32,
    pub target: u32,
    pub completion: Completion,
    /// The plan week the counter is about (the first week challenge).
    pub week: Option<u32>,
    pub next: NextStep,
    /// The plan is big enough for it (a plan with 8 topics cannot have 10
    /// mastered).
    pub reachable: bool,
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

/// A sticker won, as saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeUnlock {
    pub id: i64,
    pub challenge: ChallengeId,
    /// The plan it was won in; `None` for an overall challenge or once the
    /// plan was deleted (the sticker stays in the collection).
    pub plan: Option<PlanId>,
    /// The plan's title when the sticker was won; empty when overall.
    pub plan_title: String,
    /// The day the challenge was met, or the day it was found met when the
    /// history did not tell (`date_known` false).
    pub unlocked_on: NaiveDate,
    pub date_known: bool,
    /// The unlock notice was already shown.
    pub announced: bool,
}

/// Something counted toward a family: when (if known) and how much.
#[derive(Debug, Clone, Copy)]
struct Point {
    day: Option<NaiveDate>,
    amount: u64,
}

impl Point {
    fn on(day: NaiveDate) -> Self {
        Self {
            day: Some(day),
            amount: 1,
        }
    }

    fn undated() -> Self {
        Self {
            day: None,
            amount: 1,
        }
    }
}

/// How a family's points add up toward a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Accumulate {
    /// The total of every point.
    Sum,
    /// The largest single point (hours in one day).
    Max,
}

/// What a family measured, ready to compare with each target.
#[derive(Debug, Clone)]
struct Measure {
    points: Vec<Point>,
    accumulate: Accumulate,
    /// Amount per unit of the target (milliseconds per hour).
    unit: u64,
    /// For percent families: how many there are in the plan.
    base: Option<u32>,
    /// The most the plan allows, for counts limited by its size.
    limit: Option<u32>,
    /// Met only when this holds too (the clean notebook).
    blocked: bool,
    next: NextStep,
}

impl Measure {
    fn counting(days: impl IntoIterator<Item = NaiveDate>, next: NextStep) -> Self {
        Self {
            points: days.into_iter().map(Point::on).collect(),
            accumulate: Accumulate::Sum,
            unit: 1,
            base: None,
            limit: None,
            blocked: false,
            next,
        }
    }

    fn from_points(points: Vec<Point>, next: NextStep) -> Self {
        Self {
            points,
            ..Self::counting([], next)
        }
    }

    fn progress(&self, id: ChallengeId) -> ChallengeProgress {
        let target = match self.base {
            // Percent of what the plan has, as a count (at least one).
            Some(base) => (u64::from(id.target()) * u64::from(base))
                .div_ceil(100)
                .max(1) as u32,
            None => id.target(),
        };
        let reachable = match (self.base, self.limit) {
            (Some(base), _) => base > 0,
            (None, Some(limit)) => target <= limit,
            (None, None) => true,
        };
        let need = u64::from(target) * self.unit;
        let mut dated: Vec<Point> = self
            .points
            .iter()
            .filter(|p| p.day.is_some())
            .copied()
            .collect();
        dated.sort_by_key(|p| p.day);
        let undated: u64 = self
            .points
            .iter()
            .filter(|p| p.day.is_none())
            .map(|p| p.amount)
            .sum();
        let (value, met) = match self.accumulate {
            Accumulate::Sum => {
                let total: u64 = undated + dated.iter().map(|p| p.amount).sum::<u64>();
                let met = (total >= need).then(|| {
                    // The day it was crossed, unless undated points count.
                    if undated > 0 {
                        return None;
                    }
                    let mut sum = 0;
                    dated.iter().find_map(|p| {
                        sum += p.amount;
                        (sum >= need).then_some(p.day).flatten()
                    })
                });
                (total, met)
            }
            Accumulate::Max => {
                let best = self.points.iter().map(|p| p.amount).max().unwrap_or(0);
                let met = (best >= need)
                    .then(|| dated.iter().find(|p| p.amount >= need).and_then(|p| p.day));
                (best, met)
            }
        };
        let met = met.filter(|_| !self.blocked);
        let current = u32::try_from(value / self.unit.max(1)).unwrap_or(u32::MAX);
        ChallengeProgress {
            id,
            current: current.min(target),
            target,
            completion: match met {
                Some(day) => Completion::Met { day },
                None => Completion::Pending,
            },
            week: None,
            next: if met.is_some() {
                NextStep::None
            } else {
                self.next
            },
            reachable,
        }
    }
}

/// Every plan challenge of `plan`, in catalog order.
pub fn evaluate(plan: &StudyPlan, records: &PlanRecords) -> Vec<ChallengeProgress> {
    let study = PlanStudy::new(plan, records);
    let measures: HashMap<Family, Measure> = Family::ALL
        .into_iter()
        .filter(|f| f.scope() == Scope::Plan)
        .map(|f| (f, study.measure(f)))
        .collect();
    ChallengeId::all()
        .filter_map(|id| {
            let progress = measures.get(&id.family())?.progress(id);
            Some(study.with_week(progress))
        })
        .collect()
}

/// Every overall challenge, in catalog order.
pub fn evaluate_overall(records: &OverallRecords) -> Vec<ChallengeProgress> {
    let days: BTreeSet<NaiveDate> = records
        .study_millis
        .iter()
        .filter(|(_, ms)| *ms > 0)
        .map(|(day, _)| *day)
        .chain(records.activity_days.iter().copied())
        .collect();
    let time = |accumulate| Measure {
        points: records
            .study_millis
            .iter()
            .map(|(day, ms)| Point {
                day: Some(*day),
                amount: *ms,
            })
            .collect(),
        accumulate,
        unit: HOUR_MS,
        ..Measure::counting([], NextStep::None)
    };
    let measure = |family: Family| match family {
        Family::Hours => time(Accumulate::Sum),
        Family::LongDay => time(Accumulate::Max),
        Family::StudyDays => Measure::counting(days.iter().copied(), NextStep::None),
        Family::Plans => Measure::counting(records.plan_days.iter().copied(), NextStep::None),
        Family::AllLessons => {
            Measure::counting(records.lesson_days.iter().copied(), NextStep::None)
        }
        Family::AllQuizzes => {
            Measure::counting(records.quizzes.iter().map(|q| q.0), NextStep::None)
        }
        Family::AllRightAnswers => Measure::from_points(
            records
                .quizzes
                .iter()
                .map(|(day, right)| Point {
                    day: Some(*day),
                    amount: u64::from(*right),
                })
                .collect(),
            NextStep::None,
        ),
        Family::AllMastered => Measure::from_points(
            (0..records.mastered).map(|_| Point::undated()).collect(),
            NextStep::None,
        ),
        _ => Measure::counting(records.sticker_days.iter().copied(), NextStep::None),
    };
    ChallengeId::all()
        .filter(|id| id.scope() == Scope::Overall)
        .map(|id| measure(id.family()).progress(id))
        .collect()
}

/// One plan's study, prepared once for every rule.
struct PlanStudy<'a> {
    plan: &'a StudyPlan,
    records: &'a PlanRecords,
    /// Topics by suggested week, keeping the plan's order inside a week.
    topics: Vec<&'a Topic>,
    /// Weeks of the plan that have topics.
    week_count: u32,
    /// Complete weeks and the day each was completed, when known.
    weeks_done: Vec<Option<NaiveDate>>,
    /// The unfinished week closest to done: (done, total, week).
    closest_week: Option<(u32, u32, u32)>,
}

impl<'a> PlanStudy<'a> {
    fn new(plan: &'a StudyPlan, records: &'a PlanRecords) -> Self {
        let mut topics: Vec<&Topic> = plan.topics().map(|(_, t)| t).collect();
        topics.sort_by_key(|t| t.week);
        let mut study = Self {
            plan,
            records,
            topics,
            week_count: 0,
            weeks_done: Vec::new(),
            closest_week: None,
        };
        study.find_weeks();
        study
    }

    /// A week is done when every topic planned for it is in review or done,
    /// the same rule as the plan's check marks ([`Topic::lesson_taken`]): a
    /// lesson, or a quiz or review that put the topic into review.
    fn find_weeks(&mut self) {
        let mut weeks: Vec<u32> = self.topics.iter().map(|t| t.week).collect();
        weeks.dedup();
        self.week_count = weeks.len() as u32;
        for week in weeks {
            let planned: Vec<&Topic> = self
                .topics
                .iter()
                .copied()
                .filter(|t| t.week == week)
                .collect();
            let total = planned.len() as u32;
            let done = planned.iter().filter(|t| t.lesson_taken()).count() as u32;
            if done == total {
                // The day the last topic got there, when every one is dated
                // (a topic marked by hand has no record).
                let day = planned
                    .iter()
                    .map(|t| self.first_activity(t).flatten())
                    .collect::<Option<Vec<_>>>()
                    .and_then(|days| days.into_iter().max());
                self.weeks_done.push(day);
                continue;
            }
            let closer = self.closest_week.is_none_or(|(d, t, _)| {
                u64::from(done) * u64::from(t) > u64::from(d) * u64::from(total)
            });
            if closer {
                self.closest_week = Some((done, total, week));
            }
        }
    }

    /// When the topic was first studied (a lesson, a quiz or a graded
    /// review). `Some(None)`: studied, but before reviews were dated.
    fn first_activity(&self, topic: &Topic) -> Option<Option<NaiveDate>> {
        let r = self.records;
        let dated = [
            r.first_lesson.get(&topic.id),
            r.first_quiz.get(&topic.id),
            r.first_review.get(&topic.id),
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

    /// A lesson to take: a topic without a lesson on record, preferring one
    /// not marked as studied by hand.
    fn lesson_to_take(&self) -> NextStep {
        let open = |t: &&&Topic| !self.records.first_lesson.contains_key(&t.id);
        self.topics
            .iter()
            .filter(open)
            .find(|t| !t.lesson_taken())
            .or_else(|| self.topics.iter().find(open))
            .or_else(|| self.topics.first())
            .map_or(NextStep::None, |t| NextStep::Lesson(t.id))
    }

    /// A quiz on a studied topic, or on the first one.
    fn quiz_to_take(&self) -> NextStep {
        self.topics
            .iter()
            .find(|t| t.lesson_taken())
            .or_else(|| self.topics.first())
            .map_or(NextStep::None, |t| NextStep::Quiz(t.id))
    }

    /// The studied topic to review next: the soonest due, not mastered.
    fn review_to_take(&self) -> NextStep {
        self.topics
            .iter()
            .filter(|t| t.lesson_taken() && t.status != TopicStatus::Done)
            .min_by_key(|t| t.review.due)
            .map_or_else(|| self.lesson_to_take(), |t| NextStep::Review(t.id))
    }

    fn mistakes_step(&self) -> NextStep {
        match self.records.open_mistakes.first() {
            Some(topic) => NextStep::Mistakes(*topic),
            // No missed question yet: a quiz on a studied topic finds some.
            None => self.quiz_to_take(),
        }
    }

    /// A topic never studied: its lesson, or a quiz when it was only marked
    /// as studied by hand.
    fn untouched_step(&self) -> NextStep {
        let untouched: Vec<&&Topic> = self
            .topics
            .iter()
            .filter(|t| self.first_activity(t).is_none())
            .collect();
        match untouched.iter().find(|t| !t.lesson_taken()) {
            Some(t) => NextStep::Lesson(t.id),
            None => untouched
                .first()
                .map_or(NextStep::None, |t| NextStep::Quiz(t.id)),
        }
    }

    fn week_step(&self) -> NextStep {
        let Some((_, _, week)) = self.closest_week else {
            return NextStep::None;
        };
        self.topics
            .iter()
            .find(|t| t.week == week && !t.lesson_taken())
            .map_or(NextStep::None, |t| NextStep::Lesson(t.id))
    }

    fn mastered(&self) -> Vec<Point> {
        self.topics
            .iter()
            .filter(|t| t.status == TopicStatus::Done)
            .map(|_| Point::undated())
            .collect()
    }

    fn quizzes_where(&self, keep: impl Fn(&QuizRecord) -> bool) -> Vec<NaiveDate> {
        self.records
            .quizzes
            .iter()
            .filter(|q| keep(q))
            .map(|q| q.day)
            .collect()
    }

    fn essays_where(&self, share: f64) -> Vec<NaiveDate> {
        self.records
            .essays
            .iter()
            .filter(|e| e.max > 0.0 && e.total / e.max >= share)
            .map(|e| e.day)
            .collect()
    }

    fn mocks_where(&self, share: f64) -> Vec<NaiveDate> {
        self.records
            .mocks
            .iter()
            .filter(|(_, s)| *s >= share)
            .map(|(day, _)| *day)
            .collect()
    }

    fn measure(&self, family: Family) -> Measure {
        let r = self.records;
        let topics = self.topics.len() as u32;
        let subjects = self.plan.subjects.iter().filter(|s| !s.topics.is_empty());
        let studied = || -> Vec<Point> {
            self.topics
                .iter()
                .filter_map(|t| self.first_activity(t))
                .map(|day| Point { day, amount: 1 })
                .collect()
        };
        let limited = |mut m: Measure, limit: u32| {
            m.limit = Some(limit);
            m
        };
        let share = |mut m: Measure, base: u32| {
            m.base = Some(base);
            m
        };
        match family {
            Family::Lessons => Measure::counting(r.lesson_days.clone(), self.lesson_to_take()),
            Family::LessonDays => {
                let days: BTreeSet<NaiveDate> = r.lesson_days.iter().copied().collect();
                Measure::counting(days, self.lesson_to_take())
            }
            Family::Quizzes => {
                Measure::counting(r.quizzes.iter().map(|q| q.day), self.quiz_to_take())
            }
            Family::RightAnswers => Measure::from_points(
                r.quizzes
                    .iter()
                    .map(|q| Point {
                        day: Some(q.day),
                        amount: u64::from(q.correct),
                    })
                    .collect(),
                self.quiz_to_take(),
            ),
            Family::PerfectQuizzes => Measure::counting(
                self.quizzes_where(|q| q.total >= QUIZ_MIN_QUESTIONS && q.correct == q.total),
                self.quiz_to_take(),
            ),
            Family::HighQuizzes => Measure::counting(
                self.quizzes_where(|q| {
                    q.total >= QUIZ_MIN_QUESTIONS
                        && f64::from(q.correct) >= HIGH_QUIZ_SHARE * f64::from(q.total)
                }),
                self.quiz_to_take(),
            ),
            Family::Recoveries => {
                Measure::counting(r.recoveries.iter().map(|(_, d)| *d), self.mistakes_step())
            }
            Family::CleanNotebook => Measure {
                blocked: !r.open_mistakes.is_empty(),
                ..Measure::counting(r.recoveries.iter().map(|(_, d)| *d), self.mistakes_step())
            },
            Family::Coverage => limited(
                Measure::from_points(studied(), self.untouched_step()),
                topics,
            ),
            Family::CoverageShare => share(
                Measure::from_points(studied(), self.untouched_step()),
                topics,
            ),
            Family::Weeks => limited(
                Measure::from_points(
                    self.weeks_done
                        .iter()
                        .map(|day| Point {
                            day: *day,
                            amount: 1,
                        })
                        .collect(),
                    self.week_step(),
                ),
                self.week_count,
            ),
            Family::PlanComplete => share(
                Measure::from_points(
                    self.weeks_done
                        .iter()
                        .map(|day| Point {
                            day: *day,
                            amount: 1,
                        })
                        .collect(),
                    self.week_step(),
                ),
                self.week_count,
            ),
            Family::Mastered => limited(
                Measure::from_points(self.mastered(), self.review_to_take()),
                topics,
            ),
            Family::NoticeMastered => share(
                Measure::from_points(self.mastered(), self.review_to_take()),
                topics,
            ),
            Family::SubjectsMastered => {
                let all: Vec<_> = subjects.collect();
                let done = all
                    .iter()
                    .filter(|s| s.topics.iter().all(|t| t.status == TopicStatus::Done))
                    .map(|_| Point::undated())
                    .collect();
                limited(
                    Measure::from_points(done, self.review_to_take()),
                    all.len() as u32,
                )
            }
            Family::SubjectsStarted => {
                let all: Vec<_> = subjects.collect();
                // The day a subject's first topic was studied, when known.
                let started = all
                    .iter()
                    .filter_map(|s| {
                        let days: Vec<Option<NaiveDate>> = s
                            .topics
                            .iter()
                            .filter_map(|t| self.first_activity(t))
                            .collect();
                        if days.is_empty() {
                            return None;
                        }
                        let day = days.iter().flatten().min().copied();
                        Some(Point {
                            day: day.filter(|_| days.iter().all(Option::is_some)),
                            amount: 1,
                        })
                    })
                    .collect();
                share(
                    Measure::from_points(started, self.untouched_step()),
                    all.len() as u32,
                )
            }
            Family::Reviews => Measure::counting(r.review_days.clone(), self.review_to_take()),
            Family::PastExams => Measure::counting(r.exam_days.clone(), NextStep::Board),
            Family::HandoutsRead => Measure::from_points(
                (0..r.readings_finished).map(|_| Point::undated()).collect(),
                NextStep::Reader,
            ),
            Family::Highlights => Measure::counting(r.highlight_days.clone(), NextStep::Reader),
            Family::HandoutsMade => Measure::counting(r.handout_days.clone(), NextStep::Reader),
            Family::Essays => Measure::counting(r.essays.iter().map(|e| e.day), NextStep::Writing),
            Family::EssaysHigh => {
                Measure::counting(self.essays_where(ESSAY_HIGH_SHARE), NextStep::Writing)
            }
            Family::EssayTop => {
                Measure::counting(self.essays_where(ESSAY_TOP_SHARE), NextStep::Writing)
            }
            Family::Mocks => Measure::counting(r.mocks.iter().map(|m| m.0), NextStep::Mock),
            Family::MocksHigh => {
                Measure::counting(self.mocks_where(MOCK_HIGH_SHARE), NextStep::Mock)
            }
            Family::MockTop => Measure::counting(self.mocks_where(MOCK_TOP_SHARE), NextStep::Mock),
            // Overall families are measured by `evaluate_overall`.
            _ => Measure::counting([], NextStep::None),
        }
    }

    /// The first week challenge, before any week is done, counts the topics
    /// of the week closest to done.
    fn with_week(&self, progress: ChallengeProgress) -> ChallengeProgress {
        let first_week = progress.id.family() == Family::Weeks && progress.id.level() == 1;
        match self.closest_week {
            Some((done, total, week)) if first_week && !progress.is_met() => ChallengeProgress {
                current: done,
                target: total,
                week: Some(week),
                ..progress
            },
            _ => progress,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Difficulty, Subject, SubjectId};
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

    fn id(key: &str) -> ChallengeId {
        ChallengeId::from_key(key).unwrap()
    }

    fn get(progress: &[ChallengeProgress], key: &str) -> ChallengeProgress {
        *progress.iter().find(|p| p.id == id(key)).unwrap()
    }

    #[test]
    fn the_catalog_has_unique_keys_and_keeps_the_first_ones() {
        assert_eq!(CATALOG.len(), 164);
        let keys: BTreeSet<&str> = CATALOG.iter().map(|c| c.key).collect();
        assert_eq!(keys.len(), CATALOG.len());
        for key in [
            "first_step",
            "comeback",
            "plan_in_motion",
            "week_on_track",
            "lessons_10",
            "hours_100",
            "days_30",
        ] {
            assert!(keys.contains(key), "{key}");
        }
        for id in ChallengeId::all() {
            assert_eq!(ChallengeId::from_key(id.key()), Some(id));
        }
        assert_eq!(ChallengeId::from_key("primeiro_passo"), None);
        // Every family has challenges, lowest target first.
        for family in Family::ALL {
            let targets: Vec<u32> = family.challenges().map(ChallengeId::target).collect();
            assert!(!targets.is_empty(), "{family:?}");
            assert!(targets.windows(2).all(|w| w[0] < w[1]), "{family:?}");
        }
        assert_eq!(id("first_step").level(), 1);
        assert_eq!(id("lessons_10").level(), 4);
    }

    #[test]
    fn plan_and_overall_challenges_are_evaluated_apart() {
        let plan = plan(vec![topic(1, 1)]);
        let in_plan = evaluate(&plan, &PlanRecords::default());
        let overall = evaluate_overall(&OverallRecords::default());
        assert_eq!(in_plan.len() + overall.len(), CATALOG.len());
        assert!(in_plan.iter().all(|c| c.id.scope() == Scope::Plan));
        assert!(overall.iter().all(|c| c.id.scope() == Scope::Overall));
        assert!(in_plan.iter().chain(&overall).all(|c| !c.is_met()));
    }

    #[test]
    fn a_new_plan_points_to_its_first_lesson_and_quiz() {
        let plan = plan(vec![topic(1, 2), topic(2, 1)]);
        let progress = evaluate(&plan, &PlanRecords::default());
        assert_eq!(
            get(&progress, "first_step").next,
            NextStep::Lesson(TopicId(2))
        );
        assert_eq!(get(&progress, "comeback").next, NextStep::Quiz(TopicId(2)));
    }

    #[test]
    fn counted_challenges_are_met_on_the_day_of_the_last_one_needed() {
        let plan = plan(vec![topic(1, 1)]);
        let records = PlanRecords {
            lesson_days: (1..=12).rev().map(day).collect(),
            ..Default::default()
        };
        let progress = evaluate(&plan, &records);
        let first = get(&progress, "first_step");
        assert_eq!(first.completion, Completion::Met { day: Some(day(1)) });
        let ten = get(&progress, "lessons_10");
        assert_eq!(ten.completion, Completion::Met { day: Some(day(10)) });
        let thirty = get(&progress, "lessons_30");
        assert_eq!((thirty.current, thirty.target), (12, 30));
        assert!(!thirty.is_met());
        assert_eq!(
            get(&progress, "lesson_days_7").completion,
            Completion::Met { day: Some(day(7)) }
        );
    }

    #[test]
    fn quizzes_count_attempts_perfect_scores_and_right_answers() {
        let plan = plan(vec![topic(1, 1)]);
        let quiz = |d, correct, total| QuizRecord {
            day: day(d),
            correct,
            total,
        };
        let records = PlanRecords {
            // A perfect short quiz does not count; a perfect one of five does.
            quizzes: vec![
                quiz(2, 3, 3),
                quiz(3, 60, 70),
                quiz(5, 5, 5),
                quiz(8, 40, 50),
            ],
            ..Default::default()
        };
        let progress = evaluate(&plan, &records);
        assert_eq!(
            get(&progress, "perfect_quiz").completion,
            Completion::Met { day: Some(day(5)) }
        );
        assert_eq!(
            get(&progress, "right_answers_100").completion,
            Completion::Met { day: Some(day(8)) }
        );
        assert_eq!(get(&progress, "quizzes_10").current, 4);
        // 60/70, 5/5 and 40/50 are 80 % or more.
        assert_eq!(get(&progress, "high_quizzes_5").current, 3);
    }

    #[test]
    fn a_clean_notebook_needs_recoveries_and_no_open_mistake() {
        let plan = plan(vec![topic(1, 1)]);
        let mut records = PlanRecords {
            recoveries: (1..=5).map(|d| (TopicId(1), day(d))).collect(),
            open_mistakes: vec![TopicId(1)],
            ..Default::default()
        };
        let open = get(&evaluate(&plan, &records), "clean_notebook");
        assert!(!open.is_met());
        assert_eq!(open.next, NextStep::Mistakes(TopicId(1)));
        records.open_mistakes.clear();
        let clean = get(&evaluate(&plan, &records), "clean_notebook");
        assert_eq!(clean.completion, Completion::Met { day: Some(day(5)) });
    }

    #[test]
    fn coverage_counts_distinct_topics_once_and_shares_of_the_plan() {
        let plan = plan(vec![topic(1, 1), topic(2, 1), topic(3, 2), topic(4, 2)]);
        let mut records = PlanRecords::default();
        // Lesson and quiz on the same topic count once.
        records.first_lesson.insert(TopicId(1), day(2));
        records.first_quiz.insert(TopicId(1), day(3));
        records.first_quiz.insert(TopicId(2), day(4));
        let progress = evaluate(&plan, &records);
        let moving = get(&progress, "plan_in_motion");
        assert_eq!((moving.current, moving.target), (2, 3));
        assert_eq!(moving.next, NextStep::Lesson(TopicId(3)));
        // Half of four topics is two.
        assert_eq!(
            get(&progress, "half_notice").completion,
            Completion::Met { day: Some(day(4)) }
        );
        // Ten topics cannot be studied in a plan of four.
        assert!(!get(&progress, "coverage_10").reachable);
        records.first_review.insert(TopicId(4), day(9));
        records.first_review.insert(TopicId(3), day(7));
        let progress = evaluate(&plan, &records);
        assert_eq!(
            get(&progress, "whole_notice").completion,
            Completion::Met { day: Some(day(9)) }
        );
    }

    #[test]
    fn an_old_undated_review_counts_but_leaves_the_day_unknown() {
        let mut reviewed = topic(3, 1);
        reviewed.review.repetitions = 2;
        let plan = plan(vec![topic(1, 1), topic(2, 1), reviewed]);
        let mut records = PlanRecords::default();
        records.first_lesson.insert(TopicId(1), day(2));
        records.first_quiz.insert(TopicId(2), day(4));
        let done = get(&evaluate(&plan, &records), "plan_in_motion");
        assert_eq!(done.completion, Completion::Met { day: None });
    }

    #[test]
    fn weeks_follow_the_lesson_check_marks() {
        let mut a = topic(1, 1);
        let b = topic(2, 1);
        let c = topic(3, 2);
        a.status = TopicStatus::Reviewing;
        let mut records = PlanRecords::default();
        records.first_lesson.insert(TopicId(1), day(2));
        let progress = evaluate(&plan(vec![a.clone(), b.clone(), c.clone()]), &records);
        let week = get(&progress, "week_on_track");
        assert_eq!(week.week, Some(1));
        assert_eq!((week.current, week.target), (1, 2));
        assert_eq!(week.next, NextStep::Lesson(TopicId(2)));
        let all = get(&progress, "plan_complete");
        assert_eq!((all.current, all.target), (0, 2));
        // A plan of two weeks cannot have four done.
        assert!(!get(&progress, "four_weeks").reachable);

        let mut b = b;
        b.status = TopicStatus::Done;
        records.first_lesson.insert(TopicId(2), day(6));
        let mut c = c;
        c.status = TopicStatus::Reviewing;
        records.first_quiz.insert(TopicId(3), day(8));
        let progress = evaluate(&plan(vec![a, b, c]), &records);
        assert_eq!(
            get(&progress, "week_on_track").completion,
            Completion::Met { day: Some(day(6)) }
        );
        assert_eq!(
            get(&progress, "plan_complete").completion,
            Completion::Met { day: Some(day(8)) }
        );
    }

    #[test]
    fn a_week_marked_done_by_hand_is_met_without_a_known_day() {
        let mut a = topic(1, 1);
        a.status = TopicStatus::Done;
        let met = get(
            &evaluate(&plan(vec![a, topic(2, 2)]), &PlanRecords::default()),
            "week_on_track",
        );
        assert_eq!(met.completion, Completion::Met { day: None });
    }

    #[test]
    fn mastery_follows_done_topics_and_whole_subjects() {
        let mut a = topic(1, 1);
        a.status = TopicStatus::Done;
        let progress = evaluate(&plan(vec![a.clone(), topic(2, 1)]), &PlanRecords::default());
        assert_eq!(
            get(&progress, "first_mastery").completion,
            Completion::Met { day: None }
        );
        assert!(!get(&progress, "subject_mastered").is_met());
        let mut b = topic(2, 1);
        b.status = TopicStatus::Done;
        let progress = evaluate(&plan(vec![a, b]), &PlanRecords::default());
        assert!(get(&progress, "subject_mastered").is_met());
        assert!(get(&progress, "notice_mastered_100").is_met());
    }

    #[test]
    fn mock_exams_count_by_day_and_by_share_of_the_points() {
        let plan = plan(vec![topic(1, 1)]);
        let records = PlanRecords {
            mocks: vec![(day(5), 0.55), (day(8), 0.72), (day(12), 0.93)],
            ..Default::default()
        };
        let progress = evaluate(&plan, &records);
        assert_eq!(
            get(&progress, "first_mock").completion,
            Completion::Met { day: Some(day(5)) }
        );
        assert_eq!(
            get(&progress, "mocks_3").completion,
            Completion::Met { day: Some(day(12)) }
        );
        assert_eq!(
            get(&progress, "mocks_high_1").completion,
            Completion::Met { day: Some(day(8)) }
        );
        assert_eq!(
            get(&progress, "mock_top_1").completion,
            Completion::Met { day: Some(day(12)) }
        );
        assert!(!get(&progress, "mocks_high_3").is_met());
        assert_eq!(get(&progress, "mocks_5").next, NextStep::Mock);
    }

    #[test]
    fn materials_and_writing_point_to_their_screens() {
        let plan = plan(vec![topic(1, 1)]);
        let essay = |d, total| EssayRecord {
            day: day(d),
            total,
            max: 20.0,
        };
        let records = PlanRecords {
            exam_days: vec![day(3)],
            essays: vec![essay(4, 12.0), essay(9, 15.0), essay(10, 19.0)],
            handout_days: vec![day(2)],
            ..Default::default()
        };
        let progress = evaluate(&plan, &records);
        assert!(get(&progress, "past_exam").is_met());
        assert!(get(&progress, "handouts_made_1").is_met());
        assert_eq!(get(&progress, "handout_read").next, NextStep::Reader);
        assert_eq!(
            get(&progress, "first_essay").completion,
            Completion::Met { day: Some(day(4)) }
        );
        assert_eq!(
            get(&progress, "essay_high").completion,
            Completion::Met { day: Some(day(9)) }
        );
        assert_eq!(
            get(&progress, "essay_top_1").completion,
            Completion::Met { day: Some(day(10)) }
        );
    }

    #[test]
    fn study_time_days_and_the_collection_count_across_plans() {
        let hour = 3_600_000;
        let records = OverallRecords {
            study_millis: vec![(day(1), 6 * hour), (day(3), 5 * hour), (day(2), 0)],
            activity_days: vec![day(2), day(4), day(5), day(6), day(7), day(8)],
            plan_days: vec![day(1), day(6)],
            sticker_days: (1..=10).map(day).collect(),
            ..Default::default()
        };
        let progress = evaluate_overall(&records);
        assert_eq!(
            get(&progress, "hours_10").completion,
            Completion::Met { day: Some(day(3)) }
        );
        let fifty = get(&progress, "hours_50");
        assert_eq!((fifty.current, fifty.target), (11, 50));
        // Six hours on one day is the longest.
        assert!(get(&progress, "long_day_4").is_met());
        // Days 1 to 8: study time or an activity on each.
        assert_eq!(
            get(&progress, "days_7").completion,
            Completion::Met { day: Some(day(7)) }
        );
        assert_eq!(get(&progress, "days_30").current, 8);
        assert_eq!(
            get(&progress, "plans_2").completion,
            Completion::Met { day: Some(day(6)) }
        );
        assert!(get(&progress, "stickers_10").is_met());
    }
}
