//! SQLite persistence for plans, progress, quiz results and settings.

use std::path::Path;

use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::plan::{
    Difficulty, NewPlan, PlanId, PlanSummary, StudyPlan, Subject, SubjectId, Topic, TopicId,
    TopicStatus,
};
use crate::quiz::{QuizAttempt, QuizStats};
use crate::review::ReviewState;

/// Storage errors.
///
/// Messages are English and meant for logs; the UI shows translated text.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("plan {0:?} not found")]
    PlanNotFound(PlanId),
    #[error("corrupt value in column {column}: {value}")]
    Corrupt { column: &'static str, value: String },
}

type Result<T> = std::result::Result<T, StoreError>;

/// Schema migrations, applied in order. `PRAGMA user_version` holds how many
/// have run.
const MIGRATIONS: &[&str] = &[r"
    CREATE TABLE plans (
        id INTEGER PRIMARY KEY,
        title TEXT NOT NULL,
        exam TEXT NOT NULL,
        weeks INTEGER NOT NULL,
        created_on TEXT NOT NULL,
        edital_name TEXT NOT NULL,
        edital_text TEXT NOT NULL
    );
    CREATE TABLE subjects (
        id INTEGER PRIMARY KEY,
        plan_id INTEGER NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
        position INTEGER NOT NULL,
        name TEXT NOT NULL
    );
    CREATE TABLE topics (
        id INTEGER PRIMARY KEY,
        subject_id INTEGER NOT NULL REFERENCES subjects(id) ON DELETE CASCADE,
        position INTEGER NOT NULL,
        title TEXT NOT NULL,
        difficulty TEXT NOT NULL,
        status TEXT NOT NULL,
        week INTEGER NOT NULL,
        repetitions INTEGER NOT NULL DEFAULT 0,
        interval_days INTEGER NOT NULL DEFAULT 0,
        ease REAL NOT NULL DEFAULT 2.5,
        due TEXT,
        last_reviewed TEXT
    );
    CREATE TABLE quiz_attempts (
        id INTEGER PRIMARY KEY,
        topic_id INTEGER NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
        taken_on TEXT NOT NULL,
        correct INTEGER NOT NULL,
        total INTEGER NOT NULL
    );
    CREATE TABLE lessons (
        id INTEGER PRIMARY KEY,
        topic_id INTEGER NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
        taken_on TEXT NOT NULL
    );
    CREATE TABLE settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
"];

/// Handle to the app database.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let mut store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        let applied: usize =
            self.conn
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(applied) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
            tracing::info!(version = i + 1, "database migrated");
        }
        Ok(())
    }

    /// Stores a new plan with the edital it came from and returns it with ids.
    pub fn insert_plan(
        &mut self,
        plan: &NewPlan,
        edital_name: &str,
        edital_text: &str,
        today: NaiveDate,
    ) -> Result<StudyPlan> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO plans (title, exam, weeks, created_on, edital_name, edital_text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                plan.title(),
                plan.exam(),
                plan.weeks(),
                today.to_string(),
                edital_name,
                edital_text
            ],
        )?;
        let plan_id = PlanId(tx.last_insert_rowid());
        for (s_pos, subject) in plan.subjects().iter().enumerate() {
            tx.execute(
                "INSERT INTO subjects (plan_id, position, name) VALUES (?1, ?2, ?3)",
                params![plan_id.0, s_pos as i64, subject.name],
            )?;
            let subject_id = tx.last_insert_rowid();
            for (t_pos, topic) in subject.topics.iter().enumerate() {
                tx.execute(
                    "INSERT INTO topics (subject_id, position, title, difficulty, status, week)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        subject_id,
                        t_pos as i64,
                        topic.title,
                        topic.difficulty.as_str(),
                        TopicStatus::NotStarted.as_str(),
                        topic.week
                    ],
                )?;
            }
        }
        tx.commit()?;
        self.load_plan(plan_id)
    }

    pub fn list_plans(&self) -> Result<Vec<PlanSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.title, p.exam, p.created_on,
                    COUNT(t.id),
                    COALESCE(SUM(CASE WHEN t.status = 'done' THEN 1 ELSE 0 END), 0)
             FROM plans p
             LEFT JOIN subjects s ON s.plan_id = p.id
             LEFT JOIN topics t ON t.subject_id = s.id
             GROUP BY p.id
             ORDER BY p.id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?;
        rows.map(|row| {
            let (id, title, exam, created_on, topics, done) = row?;
            Ok(PlanSummary {
                id: PlanId(id),
                title,
                exam,
                created_on: parse_date("created_on", &created_on)?,
                topic_count: topics as usize,
                done_count: done as usize,
            })
        })
        .collect()
    }

    pub fn load_plan(&self, id: PlanId) -> Result<StudyPlan> {
        let header = self
            .conn
            .query_row(
                "SELECT title, exam, weeks, created_on FROM plans WHERE id = ?1",
                [id.0],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, u32>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((title, exam, weeks, created_on)) = header else {
            return Err(StoreError::PlanNotFound(id));
        };

        let mut subjects = Vec::new();
        let mut subject_stmt = self
            .conn
            .prepare("SELECT id, name FROM subjects WHERE plan_id = ?1 ORDER BY position")?;
        let subject_rows: Vec<(i64, String)> = subject_stmt
            .query_map([id.0], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;

        let mut topic_stmt = self.conn.prepare(
            "SELECT id, title, difficulty, status, week, repetitions, interval_days, ease,
                    due, last_reviewed
             FROM topics WHERE subject_id = ?1 ORDER BY position",
        )?;
        for (subject_id, name) in subject_rows {
            let raw: Vec<RawTopic> = topic_stmt
                .query_map([subject_id], RawTopic::from_row)?
                .collect::<std::result::Result<_, _>>()?;
            let topics = raw
                .into_iter()
                .map(RawTopic::into_topic)
                .collect::<Result<_>>()?;
            subjects.push(Subject {
                id: SubjectId(subject_id),
                name,
                topics,
            });
        }

        Ok(StudyPlan {
            id,
            title,
            exam,
            weeks,
            created_on: parse_date("created_on", &created_on)?,
            subjects,
        })
    }

    pub fn edital_text(&self, id: PlanId) -> Result<String> {
        self.conn
            .query_row("SELECT edital_text FROM plans WHERE id = ?1", [id.0], |r| {
                r.get(0)
            })
            .optional()?
            .ok_or(StoreError::PlanNotFound(id))
    }

    pub fn delete_plan(&mut self, id: PlanId) -> Result<()> {
        self.conn
            .execute("DELETE FROM plans WHERE id = ?1", [id.0])?;
        Ok(())
    }

    /// Saves a topic's status and review state.
    pub fn update_topic(&mut self, topic: &Topic) -> Result<()> {
        self.conn.execute(
            "UPDATE topics SET status = ?2, repetitions = ?3, interval_days = ?4, ease = ?5,
                    due = ?6, last_reviewed = ?7
             WHERE id = ?1",
            params![
                topic.id.0,
                topic.status.as_str(),
                topic.review.repetitions,
                topic.review.interval_days,
                f64::from(topic.review.ease),
                topic.review.due.map(|d| d.to_string()),
                topic.review.last_reviewed.map(|d| d.to_string()),
            ],
        )?;
        Ok(())
    }

    pub fn record_quiz(&mut self, attempt: &QuizAttempt) -> Result<()> {
        self.conn.execute(
            "INSERT INTO quiz_attempts (topic_id, taken_on, correct, total)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                attempt.topic_id.0,
                attempt.taken_on.to_string(),
                attempt.correct,
                attempt.total
            ],
        )?;
        Ok(())
    }

    pub fn quiz_stats(&self, plan: PlanId) -> Result<QuizStats> {
        Ok(self.conn.query_row(
            "SELECT COUNT(q.id), COALESCE(SUM(q.correct), 0), COALESCE(SUM(q.total), 0)
             FROM quiz_attempts q
             JOIN topics t ON t.id = q.topic_id
             JOIN subjects s ON s.id = t.subject_id
             WHERE s.plan_id = ?1",
            [plan.0],
            |r| {
                Ok(QuizStats {
                    attempts: r.get(0)?,
                    correct: r.get(1)?,
                    total: r.get(2)?,
                })
            },
        )?)
    }

    pub fn record_lesson(&mut self, topic: TopicId, today: NaiveDate) -> Result<()> {
        self.conn.execute(
            "INSERT INTO lessons (topic_id, taken_on) VALUES (?1, ?2)",
            params![topic.0, today.to_string()],
        )?;
        Ok(())
    }

    pub fn lesson_count(&self, plan: PlanId) -> Result<u32> {
        Ok(self.conn.query_row(
            "SELECT COUNT(l.id) FROM lessons l
             JOIN topics t ON t.id = l.topic_id
             JOIN subjects s ON s.id = t.subject_id
             WHERE s.plan_id = ?1",
            [plan.0],
            |r| r.get(0),
        )?)
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&mut self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn remove_setting(&mut self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM settings WHERE key = ?1", [key])?;
        Ok(())
    }
}

struct RawTopic {
    id: i64,
    title: String,
    difficulty: String,
    status: String,
    week: u32,
    repetitions: u32,
    interval_days: u32,
    ease: f64,
    due: Option<String>,
    last_reviewed: Option<String>,
}

impl RawTopic {
    fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            title: r.get(1)?,
            difficulty: r.get(2)?,
            status: r.get(3)?,
            week: r.get(4)?,
            repetitions: r.get(5)?,
            interval_days: r.get(6)?,
            ease: r.get(7)?,
            due: r.get(8)?,
            last_reviewed: r.get(9)?,
        })
    }

    fn into_topic(self) -> Result<Topic> {
        let status = TopicStatus::parse(&self.status).ok_or(StoreError::Corrupt {
            column: "status",
            value: self.status.clone(),
        })?;
        Ok(Topic {
            id: TopicId(self.id),
            title: self.title,
            difficulty: Difficulty::parse_lenient(&self.difficulty),
            status,
            week: self.week,
            review: ReviewState {
                repetitions: self.repetitions,
                interval_days: self.interval_days,
                ease: self.ease as f32,
                due: self
                    .due
                    .as_deref()
                    .map(|d| parse_date("due", d))
                    .transpose()?,
                last_reviewed: self
                    .last_reviewed
                    .as_deref()
                    .map(|d| parse_date("last_reviewed", d))
                    .transpose()?,
            },
        })
    }
}

fn parse_date(column: &'static str, value: &str) -> Result<NaiveDate> {
    value.parse().map_err(|_| StoreError::Corrupt {
        column,
        value: value.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{NewSubject, NewTopic};
    use crate::review::ReviewGrade;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()
    }

    fn sample_plan() -> NewPlan {
        let topic = |title: &str, week| NewTopic {
            title: title.into(),
            difficulty: Difficulty::Hard,
            week,
        };
        NewPlan::new(
            "Plano TRF",
            "TRF 3ª Região",
            8,
            vec![
                NewSubject {
                    name: "Português".into(),
                    topics: vec![topic("Crase", 1), topic("Regência", 2)],
                },
                NewSubject {
                    name: "Direito Constitucional".into(),
                    topics: vec![topic("Direitos fundamentais", 1)],
                },
            ],
        )
        .unwrap()
    }

    #[test]
    fn plan_round_trip() {
        let mut store = Store::open_in_memory().unwrap();
        let plan = store
            .insert_plan(&sample_plan(), "edital.pdf", "texto", today())
            .unwrap();
        assert_eq!(plan.subjects.len(), 2);
        assert_eq!(plan.subjects[0].topics[1].title, "Regência");
        assert_eq!(plan.subjects[0].topics[1].difficulty, Difficulty::Hard);
        assert_eq!(store.load_plan(plan.id).unwrap(), plan);
        assert_eq!(store.edital_text(plan.id).unwrap(), "texto");

        let summaries = store.list_plans().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].topic_count, 3);
    }

    #[test]
    fn topic_updates_persist() {
        let mut store = Store::open_in_memory().unwrap();
        let mut plan = store
            .insert_plan(&sample_plan(), "e.pdf", "", today())
            .unwrap();
        let topic = &mut plan.subjects[0].topics[0];
        topic.complete_lesson(today());
        topic.apply_review(ReviewGrade::Good, today());
        store.update_topic(topic).unwrap();
        assert_eq!(store.load_plan(plan.id).unwrap(), plan);
    }

    #[test]
    fn quiz_stats_and_lessons_aggregate_per_plan() {
        let mut store = Store::open_in_memory().unwrap();
        let plan = store
            .insert_plan(&sample_plan(), "e.pdf", "", today())
            .unwrap();
        let topic_id = plan.subjects[0].topics[0].id;
        for (correct, total) in [(3, 5), (5, 5)] {
            store
                .record_quiz(&QuizAttempt {
                    topic_id,
                    taken_on: today(),
                    correct,
                    total,
                })
                .unwrap();
        }
        store.record_lesson(topic_id, today()).unwrap();
        let stats = store.quiz_stats(plan.id).unwrap();
        assert_eq!((stats.attempts, stats.correct, stats.total), (2, 8, 10));
        assert_eq!(store.lesson_count(plan.id).unwrap(), 1);
    }

    #[test]
    fn deleting_a_plan_cascades() {
        let mut store = Store::open_in_memory().unwrap();
        let plan = store
            .insert_plan(&sample_plan(), "e.pdf", "", today())
            .unwrap();
        store.delete_plan(plan.id).unwrap();
        assert!(store.list_plans().unwrap().is_empty());
        assert!(matches!(
            store.load_plan(plan.id),
            Err(StoreError::PlanNotFound(_))
        ));
    }

    #[test]
    fn settings_upsert() {
        let mut store = Store::open_in_memory().unwrap();
        assert_eq!(store.setting("k").unwrap(), None);
        store.set_setting("k", "a").unwrap();
        store.set_setting("k", "b").unwrap();
        assert_eq!(store.setting("k").unwrap().as_deref(), Some("b"));
        store.remove_setting("k").unwrap();
        assert_eq!(store.setting("k").unwrap(), None);
    }

    #[test]
    fn reopening_a_file_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kg.db");
        let id = {
            let mut store = Store::open(&path).unwrap();
            store
                .insert_plan(&sample_plan(), "e.pdf", "", today())
                .unwrap()
                .id
        };
        let store = Store::open(&path).unwrap();
        assert_eq!(store.load_plan(id).unwrap().title, "Plano TRF");
    }
}
