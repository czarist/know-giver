//! Storage behind the challenges: the records their progress is computed
//! from, and the stickers won.

use std::collections::HashMap;

use chrono::NaiveDate;
use rusqlite::params;

use super::{Result, Store, StoreError, parse_date};
use crate::challenge::{
    ChallengeId, ChallengeUnlock, EssayRecord, OverallRecords, PlanRecords, QuizRecord,
};
use crate::plan::{PlanId, TopicId};

/// Rows of a plan's topic table joined up to the plan.
const IN_PLAN: &str = "JOIN topics t ON t.id = r.topic_id
     JOIN subjects s ON s.id = t.subject_id
     WHERE s.plan_id = ?1";

impl Store {
    /// Notes that a topic's review was graded on `day`. Grading it again the
    /// same day keeps one record.
    pub fn record_review(&mut self, topic: TopicId, day: NaiveDate) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO review_log (topic_id, reviewed_on) VALUES (?1, ?2)",
            params![topic.0, day.to_string()],
        )?;
        Ok(())
    }

    /// Everything the challenge rules read about one plan.
    pub fn plan_records(&self, plan: PlanId) -> Result<PlanRecords> {
        let readings_finished = self
            .readings(plan)?
            .iter()
            .filter(|r| r.page_count > 0 && r.progress() >= 1.0)
            .count();
        Ok(PlanRecords {
            first_lesson: self.first_days(plan, "lessons", "taken_on")?,
            first_quiz: self.first_days(plan, "quiz_attempts", "taken_on")?,
            first_review: self.first_days(plan, "review_log", "reviewed_on")?,
            lesson_days: self.days(
                &format!("SELECT r.taken_on FROM lessons r {IN_PLAN}"),
                plan,
                "taken_on",
            )?,
            quizzes: self.quizzes(plan)?,
            review_days: self.days(
                &format!("SELECT r.reviewed_on FROM review_log r {IN_PLAN}"),
                plan,
                "reviewed_on",
            )?,
            recoveries: self.recoveries(plan)?,
            open_mistakes: self.mistake_topics(plan)?,
            exam_days: self.days(
                "SELECT added_on FROM board_documents WHERE plan_id = ?1 AND kind = 'exam'",
                plan,
                "added_on",
            )?,
            readings_finished: u32::try_from(readings_finished).unwrap_or(u32::MAX),
            highlight_days: self.days(
                "SELECT h.created_on FROM highlights h
                 JOIN readings r ON r.id = h.reading_id WHERE r.plan_id = ?1",
                plan,
                "created_on",
            )?,
            handout_days: self.days(
                &format!("SELECT r.created_on FROM handouts r {IN_PLAN}"),
                plan,
                "created_on",
            )?,
            essays: self.essays(plan)?,
        })
    }

    /// Study across every plan: time per day and days with activity.
    pub fn overall_records(&self) -> Result<OverallRecords> {
        let mut stmt = self
            .conn
            .prepare("SELECT day, milliseconds FROM study_time ORDER BY day")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        let mut study_millis = Vec::new();
        for row in rows {
            let (day, ms) = row?;
            study_millis.push((parse_date("day", &day)?, u64::try_from(ms).unwrap_or(0)));
        }
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT day FROM activity ORDER BY day")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let activity_days = rows
            .map(|row| parse_date("day", &row?))
            .collect::<Result<_>>()?;
        let all = |sql: &str, column: &'static str| -> Result<Vec<NaiveDate>> {
            let mut stmt = self.conn.prepare(sql)?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.map(|row| parse_date(column, &row?)).collect()
        };
        let mut stmt = self
            .conn
            .prepare("SELECT taken_on, correct FROM quiz_attempts")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?;
        let mut quizzes = Vec::new();
        for row in rows {
            let (day, right) = row?;
            quizzes.push((parse_date("taken_on", &day)?, right));
        }
        let mastered: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM topics WHERE status = 'done'",
            [],
            |r| r.get(0),
        )?;
        Ok(OverallRecords {
            study_millis,
            activity_days,
            plan_days: all("SELECT created_on FROM plans", "created_on")?,
            lesson_days: all("SELECT taken_on FROM lessons", "taken_on")?,
            quizzes,
            mastered,
            // The engine fills it from the unlocks.
            sticker_days: Vec::new(),
        })
    }

    /// The day column of `sql`'s rows, for plan `?1`. `column` names it in
    /// errors.
    fn days(&self, sql: &str, plan: PlanId, column: &'static str) -> Result<Vec<NaiveDate>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([plan.0], |r| r.get::<_, String>(0))?;
        rows.map(|row| parse_date(column, &row?)).collect()
    }

    /// First day per topic in a table of dated records. `table` and
    /// `column` are fixed names from this module, never user input.
    fn first_days(
        &self,
        plan: PlanId,
        table: &'static str,
        column: &'static str,
    ) -> Result<HashMap<TopicId, NaiveDate>> {
        let sql = format!(
            "SELECT r.topic_id, MIN(r.{column}) FROM {table} r {IN_PLAN}
             GROUP BY r.topic_id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([plan.0], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (topic, day) = row?;
            Ok((TopicId(topic), parse_date(column, &day)?))
        })
        .collect()
    }

    fn quizzes(&self, plan: PlanId) -> Result<Vec<QuizRecord>> {
        let sql = format!("SELECT r.taken_on, r.correct, r.total FROM quiz_attempts r {IN_PLAN}");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([plan.0], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, u32>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (day, correct, total) = row?;
            Ok(QuizRecord {
                day: parse_date("taken_on", &day)?,
                correct,
                total,
            })
        })
        .collect()
    }

    fn recoveries(&self, plan: PlanId) -> Result<Vec<(TopicId, NaiveDate)>> {
        let sql = format!(
            "SELECT r.topic_id, r.recovered_on FROM mistake_recoveries r {IN_PLAN}
             ORDER BY r.recovered_on, r.id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([plan.0], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (topic, day) = row?;
            Ok((TopicId(topic), parse_date("recovered_on", &day)?))
        })
        .collect()
    }

    /// Topics with open mistakes, the most recently missed first.
    fn mistake_topics(&self, plan: PlanId) -> Result<Vec<TopicId>> {
        let sql = format!(
            "SELECT r.topic_id FROM mistakes r {IN_PLAN}
             GROUP BY r.topic_id
             ORDER BY MAX(r.missed_on) DESC, r.topic_id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([plan.0], |r| r.get(0).map(TopicId))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Graded written answers of a plan: the day and the estimated score.
    fn essays(&self, plan: PlanId) -> Result<Vec<EssayRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.evaluated_at, e.total, e.max_total FROM discursive_evaluations e
             JOIN discursive_attempts a ON a.id = e.attempt_id
             JOIN discursive_proposals p ON p.id = a.proposal_id
             WHERE p.plan_id = ?1",
        )?;
        let rows = stmt.query_map([plan.0], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, f64>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (at, total, max) = row?;
            // "YYYY-MM-DD HH:MM:SS": the day is the first part.
            let day = at.get(..10).unwrap_or(&at);
            Ok(EssayRecord {
                day: parse_date("evaluated_at", day)?,
                total,
                max,
            })
        })
        .collect()
    }

    /// Saves a sticker won in a plan. Returns `false` when that plan already
    /// had it, so granting twice changes nothing.
    pub fn grant_challenge(
        &mut self,
        challenge: ChallengeId,
        plan: PlanId,
        plan_title: &str,
        day: NaiveDate,
        date_known: bool,
    ) -> Result<bool> {
        let added = self.conn.execute(
            "INSERT OR IGNORE INTO challenge_unlocks
               (challenge, plan_id, plan_title, unlocked_on, date_known)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                challenge.key(),
                plan.0,
                plan_title,
                day.to_string(),
                date_known
            ],
        )?;
        Ok(added > 0)
    }

    /// Saves a sticker won across every plan (study time, study days).
    /// Returns `false` when it was already won.
    pub fn grant_overall_challenge(
        &mut self,
        challenge: ChallengeId,
        day: NaiveDate,
        date_known: bool,
    ) -> Result<bool> {
        let added = self.conn.execute(
            "INSERT OR IGNORE INTO challenge_unlocks
               (challenge, plan_id, plan_title, unlocked_on, date_known, overall)
             VALUES (?1, NULL, '', ?2, ?3, 1)",
            params![challenge.key(), day.to_string(), date_known],
        )?;
        Ok(added > 0)
    }

    /// Every sticker won, in every plan, oldest first. Keys from a newer
    /// version of the app are skipped.
    pub fn challenge_unlocks(&self) -> Result<Vec<ChallengeUnlock>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, challenge, plan_id, plan_title, unlocked_on, date_known, announced
             FROM challenge_unlocks ORDER BY unlocked_on, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, bool>(5)?,
                r.get::<_, bool>(6)?,
            ))
        })?;
        let mut unlocks = Vec::new();
        for row in rows {
            let (id, key, plan, plan_title, day, date_known, announced) = row?;
            let Some(challenge) = ChallengeId::from_key(&key) else {
                tracing::debug!(key, "unknown challenge key; skipped");
                continue;
            };
            unlocks.push(ChallengeUnlock {
                id,
                challenge,
                plan: plan.map(PlanId),
                plan_title,
                unlocked_on: parse_date("unlocked_on", &day)?,
                date_known,
                announced,
            });
        }
        Ok(unlocks)
    }

    /// The unlock notices were shown; they are not shown again.
    pub fn mark_unlocks_announced(&mut self, ids: &[i64]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for id in ids {
            let changed = tx.execute(
                "UPDATE challenge_unlocks SET announced = 1 WHERE id = ?1",
                [id],
            )?;
            if changed == 0 {
                return Err(StoreError::Corrupt {
                    column: "challenge_unlocks.id",
                    value: id.to_string(),
                });
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Difficulty, NewPlan, NewSubject, NewTopic};
    use crate::quiz::{QuizAttempt, QuizQuestion};

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn new_plan(title: &str) -> NewPlan {
        let topic = |title: &str, week| NewTopic {
            title: title.into(),
            difficulty: Difficulty::Medium,
            week,
        };
        NewPlan::new(
            title,
            "Exam",
            4,
            vec![NewSubject {
                name: "Português".into(),
                topics: vec![topic("Crase", 1), topic("Regência", 1), topic("Vírgula", 2)],
            }],
        )
        .unwrap()
    }

    fn question() -> QuizQuestion {
        QuizQuestion {
            prompt: "Há crase em 'vou à escola'?".into(),
            options: vec!["Sim".into(), "Não".into()],
            correct: 0,
            explanation: String::new(),
            from_board: false,
        }
    }

    #[test]
    fn records_are_kept_per_plan() {
        let mut store = Store::open_in_memory().unwrap();
        let a = store
            .insert_plan(&new_plan("A"), "a.pdf", "", day(1))
            .unwrap();
        let b = store
            .insert_plan(&new_plan("B"), "b.pdf", "", day(1))
            .unwrap();
        let topic_a = a.subjects[0].topics[0].id;
        let topic_b = b.subjects[0].topics[1].id;
        store
            .record_lesson(topic_a, day(3), &Default::default())
            .unwrap();
        store
            .record_lesson(topic_a, day(2), &Default::default())
            .unwrap();
        store
            .record_quiz(&QuizAttempt {
                topic_id: topic_b,
                taken_on: day(4),
                correct: 1,
                total: 2,
            })
            .unwrap();
        let records_a = store.plan_records(a.id).unwrap();
        assert_eq!(records_a.first_lesson.get(&topic_a), Some(&day(2)));
        assert_eq!(records_a.lesson_days.len(), 2);
        assert!(records_a.quizzes.is_empty());
        let records_b = store.plan_records(b.id).unwrap();
        assert!(records_b.first_lesson.is_empty());
        assert_eq!(records_b.first_quiz.get(&topic_b), Some(&day(4)));
        assert_eq!(
            records_b.quizzes,
            [QuizRecord {
                day: day(4),
                correct: 1,
                total: 2
            }]
        );
    }

    #[test]
    fn a_recovered_mistake_counts_once() {
        let mut store = Store::open_in_memory().unwrap();
        let plan = store
            .insert_plan(&new_plan("A"), "a.pdf", "", day(1))
            .unwrap();
        let topic = plan.subjects[0].topics[0].id;
        store.record_mistake(topic, &question(), 1, day(2)).unwrap();
        assert_eq!(store.plan_records(plan.id).unwrap().open_mistakes, [topic]);
        assert!(
            store
                .clear_mistake(topic, &question().prompt, day(5))
                .unwrap()
        );
        // The same right answer handled again finds nothing to count.
        assert!(
            !store
                .clear_mistake(topic, &question().prompt, day(5))
                .unwrap()
        );
        let records = store.plan_records(plan.id).unwrap();
        assert_eq!(records.recoveries, [(topic, day(5))]);
        assert!(records.open_mistakes.is_empty());
    }

    #[test]
    fn reviews_keep_one_record_a_day() {
        let mut store = Store::open_in_memory().unwrap();
        let plan = store
            .insert_plan(&new_plan("A"), "a.pdf", "", day(1))
            .unwrap();
        let topic = plan.subjects[0].topics[2].id;
        store.record_review(topic, day(6)).unwrap();
        store.record_review(topic, day(6)).unwrap();
        store.record_review(topic, day(8)).unwrap();
        let records = store.plan_records(plan.id).unwrap();
        assert_eq!(records.first_review.get(&topic), Some(&day(6)));
        assert_eq!(records.review_days.len(), 2);
    }

    #[test]
    fn overall_records_sum_study_time_and_activity_days() {
        let mut store = Store::open_in_memory().unwrap();
        store.add_study_millis(day(2), 1_000).unwrap();
        store.add_study_millis(day(2), 500).unwrap();
        store
            .record_activity(crate::ActivityKind::Lesson, day(3))
            .unwrap();
        store
            .record_activity(crate::ActivityKind::Quiz, day(3))
            .unwrap();
        let records = store.overall_records().unwrap();
        assert_eq!(records.study_millis, [(day(2), 1_500)]);
        assert!(records.plan_days.is_empty() && records.lesson_days.is_empty());
        assert_eq!(records.activity_days, [day(3)]);
    }

    #[test]
    fn a_sticker_is_won_once_per_plan_and_survives_the_plan() {
        let mut store = Store::open_in_memory().unwrap();
        let a = store
            .insert_plan(&new_plan("A"), "a.pdf", "", day(1))
            .unwrap();
        let b = store
            .insert_plan(&new_plan("B"), "b.pdf", "", day(1))
            .unwrap();
        let first = ChallengeId::from_key("first_step").unwrap();
        assert!(
            store
                .grant_challenge(first, a.id, "A", day(2), true)
                .unwrap()
        );
        assert!(
            !store
                .grant_challenge(first, a.id, "A", day(9), true)
                .unwrap()
        );
        assert!(
            store
                .grant_challenge(first, b.id, "B", day(3), false)
                .unwrap()
        );
        let unlocks = store.challenge_unlocks().unwrap();
        assert_eq!(unlocks.len(), 2);
        assert_eq!(unlocks[0].unlocked_on, day(2));
        assert!(!unlocks[1].date_known);

        store.mark_unlocks_announced(&[unlocks[0].id]).unwrap();
        store.delete_plan(a.id).unwrap();
        let unlocks = store.challenge_unlocks().unwrap();
        assert_eq!(unlocks.len(), 2);
        assert_eq!((unlocks[0].plan, unlocks[0].announced), (None, true));
        assert_eq!(unlocks[0].plan_title, "A");
        // A plan created after the deletion starts with nothing won.
        let c = store
            .insert_plan(&new_plan("C"), "c.pdf", "", day(4))
            .unwrap();
        assert!(
            store
                .grant_challenge(first, c.id, "C", day(5), true)
                .unwrap()
        );
    }

    #[test]
    fn an_overall_sticker_is_won_once() {
        let mut store = Store::open_in_memory().unwrap();
        let hours = ChallengeId::from_key("hours_10").unwrap();
        assert!(store.grant_overall_challenge(hours, day(2), true).unwrap());
        assert!(!store.grant_overall_challenge(hours, day(4), true).unwrap());
        let unlocks = store.challenge_unlocks().unwrap();
        assert_eq!(unlocks.len(), 1);
        assert_eq!((unlocks[0].plan, unlocks[0].unlocked_on), (None, day(2)));
    }

    #[test]
    fn migrates_a_database_with_plans_from_before_challenges() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        let topic = {
            let mut store = Store::open(&path).unwrap();
            let plan = store
                .insert_plan(&new_plan("A"), "a.pdf", "", day(1))
                .unwrap();
            let topic = plan.subjects[0].topics[0].id;
            store
                .record_lesson(topic, day(2), &Default::default())
                .unwrap();
            // Back to the schema of 0.37: the challenge tables did not exist.
            store
                .conn
                .execute_batch(
                    "DROP TABLE mistake_recoveries; DROP TABLE review_log;
                     DROP TABLE challenge_unlocks; PRAGMA user_version = 9;",
                )
                .unwrap();
            topic
        };
        let store = Store::open(&path).unwrap();
        let version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version as usize, super::super::MIGRATIONS.len());
        let plan = store.list_plans().unwrap()[0].id;
        let records = store.plan_records(plan).unwrap();
        assert_eq!(records.first_lesson.get(&topic), Some(&day(2)));
        assert!(store.challenge_unlocks().unwrap().is_empty());
    }
}
