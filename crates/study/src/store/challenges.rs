//! Storage behind the challenges: the records their progress is computed
//! from, and the stickers won.

use std::collections::HashMap;

use chrono::NaiveDate;
use rusqlite::params;

use super::{Result, Store, StoreError, parse_date};
use crate::challenge::{ChallengeId, ChallengeUnlock, PlanRecords};
use crate::plan::{PlanId, TopicId};

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
        Ok(PlanRecords {
            first_lesson: self.first_days(plan, "lessons", "taken_on")?,
            first_quiz: self.first_days(plan, "quiz_attempts", "taken_on")?,
            first_review: self.first_days(plan, "review_log", "reviewed_on")?,
            recoveries: self.recoveries(plan)?,
            open_mistakes: self.mistake_topics(plan)?,
        })
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
            "SELECT r.topic_id, MIN(r.{column}) FROM {table} r
             JOIN topics t ON t.id = r.topic_id
             JOIN subjects s ON s.id = t.subject_id
             WHERE s.plan_id = ?1
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

    fn recoveries(&self, plan: PlanId) -> Result<Vec<(TopicId, NaiveDate)>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.topic_id, r.recovered_on FROM mistake_recoveries r
             JOIN topics t ON t.id = r.topic_id
             JOIN subjects s ON s.id = t.subject_id
             WHERE s.plan_id = ?1
             ORDER BY r.recovered_on, r.id",
        )?;
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
        let mut stmt = self.conn.prepare(
            "SELECT m.topic_id FROM mistakes m
             JOIN topics t ON t.id = m.topic_id
             JOIN subjects s ON s.id = t.subject_id
             WHERE s.plan_id = ?1
             GROUP BY m.topic_id
             ORDER BY MAX(m.missed_on) DESC, m.topic_id",
        )?;
        let rows = stmt.query_map([plan.0], |r| r.get(0).map(TopicId))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
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
        assert!(records_a.first_quiz.is_empty());
        let records_b = store.plan_records(b.id).unwrap();
        assert!(records_b.first_lesson.is_empty());
        assert_eq!(records_b.first_quiz.get(&topic_b), Some(&day(4)));
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
        let first = ChallengeId::FirstStep;
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
