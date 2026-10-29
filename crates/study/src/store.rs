//! SQLite persistence for plans, progress, quiz results and settings.

use std::path::Path;

use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::activity::ActivityKind;
use crate::board::{
    BoardDocument, BoardExcerpt, BoardQuestion, BoardQuestionId, DocumentId, DocumentKind,
    ExcerptId, NewBoardExcerpt, NewBoardQuestion, Relevance,
};
use crate::notebook::{LessonRecap, Mistake, MistakeId};
use crate::plan::{
    Difficulty, NewPlan, PlanId, PlanSummary, StudyPlan, Subject, SubjectId, Topic, TopicId,
    TopicStatus,
};
use crate::quiz::{QuizAttempt, QuizQuestion, QuizStats};
use crate::reading::{Highlight, HighlightColor, HighlightId, Reading, ReadingId};
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
const MIGRATIONS: &[&str] = &[
    r"
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
",
    r"
    CREATE TABLE handouts (
        topic_id INTEGER PRIMARY KEY REFERENCES topics(id) ON DELETE CASCADE,
        content TEXT NOT NULL,
        created_on TEXT NOT NULL
    );
",
    r"
    CREATE TABLE board_documents (
        id INTEGER PRIMARY KEY,
        plan_id INTEGER NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
        kind TEXT NOT NULL,
        file_name TEXT NOT NULL,
        text TEXT NOT NULL,
        added_on TEXT NOT NULL
    );
    CREATE TABLE board_questions (
        id INTEGER PRIMARY KEY,
        document_id INTEGER NOT NULL REFERENCES board_documents(id) ON DELETE CASCADE,
        number INTEGER,
        prompt TEXT NOT NULL,
        options TEXT NOT NULL,
        correct INTEGER,
        topic_id INTEGER REFERENCES topics(id) ON DELETE SET NULL,
        relevance TEXT NOT NULL,
        reason TEXT NOT NULL
    );
    CREATE TABLE board_excerpts (
        id INTEGER PRIMARY KEY,
        document_id INTEGER NOT NULL REFERENCES board_documents(id) ON DELETE CASCADE,
        summary TEXT NOT NULL,
        topic_id INTEGER REFERENCES topics(id) ON DELETE SET NULL,
        relevance TEXT NOT NULL,
        reason TEXT NOT NULL
    );
    CREATE INDEX board_questions_topic ON board_questions(topic_id);
    CREATE INDEX board_excerpts_topic ON board_excerpts(topic_id);
",
    r"
    CREATE TABLE activity (
        id INTEGER PRIMARY KEY,
        day TEXT NOT NULL,
        kind TEXT NOT NULL
    );
    CREATE INDEX activity_day ON activity(day);
",
    r"
    CREATE TABLE study_time (
        day TEXT PRIMARY KEY,
        milliseconds INTEGER NOT NULL DEFAULT 0 CHECK(milliseconds >= 0)
    );
",
    r"
    CREATE TABLE quiz_banks (
        topic_id INTEGER PRIMARY KEY REFERENCES topics(id) ON DELETE CASCADE,
        content TEXT NOT NULL,
        created_on TEXT NOT NULL
    );
",
    r"
    CREATE TABLE readings (
        id INTEGER PRIMARY KEY,
        plan_id INTEGER NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
        title TEXT NOT NULL,
        path TEXT NOT NULL,
        document TEXT NOT NULL,
        created_on TEXT NOT NULL,
        page_count INTEGER NOT NULL DEFAULT 0,
        last_page INTEGER NOT NULL DEFAULT 0,
        pages_seen TEXT NOT NULL DEFAULT ''
    );
    CREATE TABLE highlights (
        id INTEGER PRIMARY KEY,
        reading_id INTEGER NOT NULL REFERENCES readings(id) ON DELETE CASCADE,
        page INTEGER NOT NULL,
        first_box INTEGER NOT NULL,
        last_box INTEGER NOT NULL,
        text TEXT NOT NULL,
        color TEXT NOT NULL,
        note TEXT NOT NULL DEFAULT '',
        created_on TEXT NOT NULL
    );
",
    r"
    CREATE TABLE mistakes (
        id INTEGER PRIMARY KEY,
        topic_id INTEGER NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
        prompt TEXT NOT NULL,
        options TEXT NOT NULL,
        correct INTEGER NOT NULL,
        explanation TEXT NOT NULL,
        from_board INTEGER NOT NULL,
        chosen INTEGER NOT NULL,
        missed_on TEXT NOT NULL,
        misses INTEGER NOT NULL DEFAULT 1,
        UNIQUE(topic_id, prompt)
    );
    CREATE TABLE lesson_drafts (
        topic_id INTEGER PRIMARY KEY REFERENCES topics(id) ON DELETE CASCADE,
        content TEXT NOT NULL,
        saved_on TEXT NOT NULL
    );
    ALTER TABLE lessons ADD COLUMN outline TEXT NOT NULL DEFAULT '[]';
    ALTER TABLE lessons ADD COLUMN board TEXT NOT NULL DEFAULT '[]';
    ALTER TABLE lessons ADD COLUMN summary TEXT NOT NULL DEFAULT '';
",
    r"
    CREATE TABLE discursive_proposals (
        id INTEGER PRIMARY KEY,
        plan_id INTEGER NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
        kind TEXT NOT NULL,
        difficulty TEXT NOT NULL,
        theme TEXT NOT NULL,
        statement TEXT NOT NULL,
        instructions TEXT NOT NULL,
        topics TEXT NOT NULL,
        line_limit INTEGER,
        minutes INTEGER,
        rubric TEXT NOT NULL,
        expected TEXT NOT NULL,
        model TEXT NOT NULL,
        prompt_version TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE TABLE discursive_attempts (
        id INTEGER PRIMARY KEY,
        proposal_id INTEGER NOT NULL REFERENCES discursive_proposals(id) ON DELETE CASCADE,
        number INTEGER NOT NULL,
        parent_id INTEGER REFERENCES discursive_attempts(id) ON DELETE SET NULL,
        origin TEXT NOT NULL,
        status TEXT NOT NULL,
        failed_stage TEXT,
        note TEXT NOT NULL DEFAULT '',
        run INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE TABLE discursive_pages (
        id INTEGER PRIMARY KEY,
        attempt_id INTEGER NOT NULL REFERENCES discursive_attempts(id) ON DELETE CASCADE,
        position INTEGER NOT NULL,
        original_path TEXT NOT NULL,
        processed_path TEXT NOT NULL,
        rotation INTEGER NOT NULL DEFAULT 0,
        width INTEGER NOT NULL,
        height INTEGER NOT NULL,
        issues TEXT NOT NULL
    );
    CREATE TABLE discursive_transcripts (
        attempt_id INTEGER PRIMARY KEY REFERENCES discursive_attempts(id) ON DELETE CASCADE,
        original TEXT NOT NULL,
        confirmed TEXT,
        model TEXT NOT NULL,
        prompt_version TEXT NOT NULL,
        created_at TEXT NOT NULL,
        confirmed_at TEXT
    );
    CREATE TABLE discursive_evaluations (
        attempt_id INTEGER PRIMARY KEY REFERENCES discursive_attempts(id) ON DELETE CASCADE,
        content TEXT NOT NULL,
        rubric_key TEXT NOT NULL,
        total REAL NOT NULL,
        max_total REAL NOT NULL,
        model TEXT NOT NULL,
        prompt_version TEXT NOT NULL,
        evaluated_at TEXT NOT NULL
    );
    CREATE TABLE discursive_difficulties (
        plan_id INTEGER PRIMARY KEY REFERENCES plans(id) ON DELETE CASCADE,
        issues TEXT NOT NULL,
        attempts TEXT NOT NULL,
        model TEXT NOT NULL,
        prompt_version TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE INDEX discursive_attempts_proposal ON discursive_attempts(proposal_id);
    CREATE INDEX discursive_pages_attempt ON discursive_pages(attempt_id, position);
",
];

mod discursive;

/// Handle to the app database.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Adds active study time to a local calendar day.
    pub fn add_study_millis(&mut self, day: NaiveDate, milliseconds: u64) -> Result<()> {
        let milliseconds = i64::try_from(milliseconds).map_err(|_| StoreError::Corrupt {
            column: "milliseconds",
            value: milliseconds.to_string(),
        })?;
        self.conn.execute(
            "INSERT INTO study_time (day, milliseconds) VALUES (?1, ?2)
             ON CONFLICT(day) DO UPDATE SET milliseconds = study_time.milliseconds + excluded.milliseconds",
            params![day.to_string(), milliseconds],
        )?;
        Ok(())
    }

    /// Active study time already saved for a local calendar day.
    pub fn study_millis(&self, day: NaiveDate) -> Result<u64> {
        let value: Option<i64> = self
            .conn
            .query_row(
                "SELECT milliseconds FROM study_time WHERE day = ?1",
                [day.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        u64::try_from(value.unwrap_or(0)).map_err(|_| StoreError::Corrupt {
            column: "milliseconds",
            value: value.unwrap_or(0).to_string(),
        })
    }

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

    /// Replaces the notice a plan was made from, keeping its topics,
    /// progress and everything made for it.
    pub fn update_edital(&mut self, id: PlanId, name: &str, text: &str) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE plans SET edital_name = ?2, edital_text = ?3 WHERE id = ?1",
            params![id.0, name, text],
        )?;
        if changed == 0 {
            return Err(StoreError::PlanNotFound(id));
        }
        Ok(())
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

    /// Records a lesson taken today and what it covered.
    pub fn record_lesson(
        &mut self,
        topic: TopicId,
        today: NaiveDate,
        recap: &LessonRecap,
    ) -> Result<()> {
        let json = |items: &[String]| serde_json::to_string(items).unwrap_or_else(|_| "[]".into());
        self.conn.execute(
            "INSERT INTO lessons (topic_id, taken_on, outline, board, summary)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                topic.0,
                today.to_string(),
                json(&recap.outline),
                json(&recap.board),
                recap.summary
            ],
        )?;
        Ok(())
    }

    /// What the latest lesson on a topic covered, if any was taken.
    pub fn last_lesson(&self, topic: TopicId) -> Result<Option<LessonRecap>> {
        let row = self
            .conn
            .query_row(
                "SELECT taken_on, outline, board, summary FROM lessons
                 WHERE topic_id = ?1 ORDER BY id DESC LIMIT 1",
                [topic.0],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((taken_on, outline, board, summary)) = row else {
            return Ok(None);
        };
        Ok(Some(LessonRecap {
            taken_on: Some(parse_date("taken_on", &taken_on)?),
            outline: serde_json::from_str(&outline).unwrap_or_default(),
            board: serde_json::from_str(&board).unwrap_or_default(),
            summary,
        }))
    }

    /// Keeps an unfinished lesson (engine JSON) to pick it up later.
    pub fn save_lesson_draft(
        &mut self,
        topic: TopicId,
        content: &str,
        today: NaiveDate,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO lesson_drafts (topic_id, content, saved_on) VALUES (?1, ?2, ?3)
             ON CONFLICT(topic_id) DO UPDATE SET content = excluded.content,
                 saved_on = excluded.saved_on",
            params![topic.0, content, today.to_string()],
        )?;
        Ok(())
    }

    pub fn lesson_draft(&self, topic: TopicId) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT content FROM lesson_drafts WHERE topic_id = ?1",
                [topic.0],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn delete_lesson_draft(&mut self, topic: TopicId) -> Result<()> {
        self.conn
            .execute("DELETE FROM lesson_drafts WHERE topic_id = ?1", [topic.0])?;
        Ok(())
    }

    /// Topics of a plan with an unfinished lesson.
    pub fn lesson_draft_topics(&self, plan: PlanId) -> Result<Vec<TopicId>> {
        let mut stmt = self.conn.prepare(
            "SELECT d.topic_id FROM lesson_drafts d
             JOIN topics t ON t.id = d.topic_id
             JOIN subjects s ON s.id = t.subject_id
             WHERE s.plan_id = ?1",
        )?;
        let rows = stmt.query_map([plan.0], |r| r.get(0).map(TopicId))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Notes a wrong quiz answer. Missing the same question again updates it.
    pub fn record_mistake(
        &mut self,
        topic: TopicId,
        question: &QuizQuestion,
        chosen: usize,
        today: NaiveDate,
    ) -> Result<()> {
        let options = serde_json::to_string(&question.options).unwrap_or_else(|_| "[]".into());
        self.conn.execute(
            "INSERT INTO mistakes
               (topic_id, prompt, options, correct, explanation, from_board, chosen, missed_on)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(topic_id, prompt) DO UPDATE SET options = excluded.options,
                 correct = excluded.correct, explanation = excluded.explanation,
                 chosen = excluded.chosen, missed_on = excluded.missed_on,
                 misses = mistakes.misses + 1",
            params![
                topic.0,
                question.prompt,
                options,
                question.correct as i64,
                question.explanation,
                question.from_board,
                chosen as i64,
                today.to_string()
            ],
        )?;
        Ok(())
    }

    /// The question was answered right: it leaves the notebook.
    pub fn clear_mistake(&mut self, topic: TopicId, prompt: &str) -> Result<bool> {
        let removed = self.conn.execute(
            "DELETE FROM mistakes WHERE topic_id = ?1 AND prompt = ?2",
            params![topic.0, prompt],
        )?;
        Ok(removed > 0)
    }

    pub fn delete_mistake(&mut self, id: MistakeId) -> Result<()> {
        self.conn
            .execute("DELETE FROM mistakes WHERE id = ?1", [id.0])?;
        Ok(())
    }

    /// Every open mistake of a plan, latest first.
    pub fn mistakes(&self, plan: PlanId) -> Result<Vec<Mistake>> {
        self.query_mistakes(
            "JOIN topics t ON t.id = m.topic_id
             JOIN subjects s ON s.id = t.subject_id
             WHERE s.plan_id = ?1",
            plan.0,
        )
    }

    /// Open mistakes on one topic, latest first.
    pub fn topic_mistakes(&self, topic: TopicId) -> Result<Vec<Mistake>> {
        self.query_mistakes("WHERE m.topic_id = ?1", topic.0)
    }

    fn query_mistakes(&self, filter: &str, id: i64) -> Result<Vec<Mistake>> {
        let sql = format!(
            "SELECT m.id, m.topic_id, m.prompt, m.options, m.correct, m.explanation,
                    m.from_board, m.chosen, m.missed_on, m.misses
             FROM mistakes m {filter}
             ORDER BY m.missed_on DESC, m.id DESC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([id], |r| {
            Ok((
                Mistake {
                    id: MistakeId(r.get(0)?),
                    topic_id: TopicId(r.get(1)?),
                    question: QuizQuestion {
                        prompt: r.get(2)?,
                        options: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_default(),
                        correct: r.get::<_, i64>(4)?.max(0) as usize,
                        explanation: r.get(5)?,
                        from_board: r.get(6)?,
                    },
                    chosen: r.get::<_, i64>(7)?.max(0) as usize,
                    missed_on: NaiveDate::MIN,
                    misses: r.get::<_, i64>(9)?.max(0) as u32,
                },
                r.get::<_, String>(8)?,
            ))
        })?;
        let mut mistakes = Vec::new();
        for row in rows {
            let (mut mistake, day) = row?;
            mistake.missed_on = parse_date("missed_on", &day)?;
            // A question whose options were lost cannot be asked again.
            if mistake.question.correct < mistake.question.options.len() {
                mistakes.push(mistake);
            }
        }
        Ok(mistakes)
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

    /// Cached handout text (JSON) for a topic.
    pub fn handout(&self, topic: TopicId) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT content FROM handouts WHERE topic_id = ?1",
                [topic.0],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn save_handout(&mut self, topic: TopicId, content: &str, today: NaiveDate) -> Result<()> {
        self.conn.execute(
            "INSERT INTO handouts (topic_id, content, created_on) VALUES (?1, ?2, ?3)
             ON CONFLICT(topic_id) DO UPDATE SET content = excluded.content,
                 created_on = excluded.created_on",
            params![topic.0, content, today.to_string()],
        )?;
        Ok(())
    }

    /// Questions prepared in advance for a topic's quizzes (JSON).
    /// Keeps a generated handout for reading. `document` is its content as
    /// JSON, to lay the pages out again.
    pub fn insert_reading(
        &mut self,
        plan: PlanId,
        title: &str,
        path: &str,
        document: &str,
        today: NaiveDate,
    ) -> Result<ReadingId> {
        self.conn.execute(
            "INSERT INTO readings (plan_id, title, path, document, created_on)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![plan.0, title, path, document, today.to_string()],
        )?;
        Ok(ReadingId(self.conn.last_insert_rowid()))
    }

    /// Handouts kept for reading in a plan, newest first.
    pub fn readings(&self, plan: PlanId) -> Result<Vec<Reading>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, path, created_on, page_count, last_page, pages_seen
             FROM readings WHERE plan_id = ?1 ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([plan.0], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        rows.map(|row| {
            let (id, title, path, created_on, page_count, last_page, seen) = row?;
            Ok(Reading {
                id: ReadingId(id),
                plan_id: plan,
                title,
                path,
                created_on: parse_date("created_on", &created_on)?,
                page_count: usize::try_from(page_count).unwrap_or(0),
                last_page: usize::try_from(last_page).unwrap_or(0),
                pages_seen: crate::reading::pages_from_text(&seen),
            })
        })
        .collect()
    }

    /// The handout's content (JSON) for laying its pages out.
    pub fn reading_document(&self, id: ReadingId) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT document FROM readings WHERE id = ?1", [id.0], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn save_reading_progress(&mut self, reading: &Reading) -> Result<()> {
        self.conn.execute(
            "UPDATE readings SET page_count = ?2, last_page = ?3, pages_seen = ?4 WHERE id = ?1",
            params![
                reading.id.0,
                reading.page_count as i64,
                reading.last_page as i64,
                crate::reading::pages_to_text(&reading.pages_seen)
            ],
        )?;
        Ok(())
    }

    pub fn delete_reading(&mut self, id: ReadingId) -> Result<()> {
        self.conn
            .execute("DELETE FROM readings WHERE id = ?1", [id.0])?;
        Ok(())
    }

    pub fn insert_highlight(
        &mut self,
        reading: ReadingId,
        highlight: &Highlight,
        today: NaiveDate,
    ) -> Result<HighlightId> {
        self.conn.execute(
            "INSERT INTO highlights (reading_id, page, first_box, last_box, text, color, note, created_on)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                reading.0,
                highlight.page as i64,
                highlight.first as i64,
                highlight.last as i64,
                highlight.text,
                highlight.color.as_str(),
                highlight.note,
                today.to_string()
            ],
        )?;
        Ok(HighlightId(self.conn.last_insert_rowid()))
    }

    /// Highlights of a handout, in page and reading order.
    pub fn highlights(&self, reading: ReadingId) -> Result<Vec<Highlight>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, page, first_box, last_box, text, color, note FROM highlights
             WHERE reading_id = ?1 ORDER BY page, first_box",
        )?;
        let rows = stmt.query_map([reading.0], |r| {
            Ok(Highlight {
                id: HighlightId(r.get(0)?),
                page: usize::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                first: usize::try_from(r.get::<_, i64>(2)?).unwrap_or(0),
                last: usize::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
                text: r.get(4)?,
                color: HighlightColor::parse(&r.get::<_, String>(5)?),
                note: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn update_highlight(
        &mut self,
        id: HighlightId,
        color: HighlightColor,
        note: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE highlights SET color = ?2, note = ?3 WHERE id = ?1",
            params![id.0, color.as_str(), note],
        )?;
        Ok(())
    }

    pub fn delete_highlight(&mut self, id: HighlightId) -> Result<()> {
        self.conn
            .execute("DELETE FROM highlights WHERE id = ?1", [id.0])?;
        Ok(())
    }

    pub fn quiz_bank(&self, topic: TopicId) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT content FROM quiz_banks WHERE topic_id = ?1",
                [topic.0],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn save_quiz_bank(
        &mut self,
        topic: TopicId,
        content: &str,
        today: NaiveDate,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO quiz_banks (topic_id, content, created_on) VALUES (?1, ?2, ?3)
             ON CONFLICT(topic_id) DO UPDATE SET content = excluded.content,
                 created_on = excluded.created_on",
            params![topic.0, content, today.to_string()],
        )?;
        Ok(())
    }

    /// How many topics of a plan already have a cached handout.
    pub fn handout_count(&self, plan: PlanId) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(h.topic_id) FROM handouts h
             JOIN topics t ON t.id = h.topic_id
             JOIN subjects s ON s.id = t.subject_id
             WHERE s.plan_id = ?1",
            [plan.0],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    /// Stores an uploaded document with what was taken from it.
    #[expect(
        clippy::too_many_arguments,
        reason = "one row plus its children, inserted together"
    )]
    pub fn insert_board_document(
        &mut self,
        plan: PlanId,
        kind: DocumentKind,
        file_name: &str,
        text: &str,
        questions: &[NewBoardQuestion],
        excerpts: &[NewBoardExcerpt],
        today: NaiveDate,
    ) -> Result<DocumentId> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO board_documents (plan_id, kind, file_name, text, added_on)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![plan.0, kind.as_str(), file_name, text, today.to_string()],
        )?;
        let doc = tx.last_insert_rowid();
        for q in questions {
            let options = serde_json::to_string(&q.options).unwrap_or_else(|_| "[]".into());
            tx.execute(
                "INSERT INTO board_questions
                   (document_id, number, prompt, options, correct, topic_id, relevance, reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    doc,
                    q.number,
                    q.prompt,
                    options,
                    q.correct.map(|c| c as i64),
                    q.topic_id.map(|t| t.0),
                    q.relevance.as_str(),
                    q.reason
                ],
            )?;
        }
        for e in excerpts {
            tx.execute(
                "INSERT INTO board_excerpts (document_id, summary, topic_id, relevance, reason)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    doc,
                    e.summary,
                    e.topic_id.map(|t| t.0),
                    e.relevance.as_str(),
                    e.reason
                ],
            )?;
        }
        tx.commit()?;
        Ok(DocumentId(doc))
    }

    pub fn board_documents(&self, plan: PlanId) -> Result<Vec<BoardDocument>> {
        let mut stmt = self.conn.prepare(
            "SELECT d.id, d.kind, d.file_name, d.added_on,
               (SELECT COUNT(*) FROM board_questions q WHERE q.document_id = d.id
                  AND q.relevance != 'out')
             + (SELECT COUNT(*) FROM board_excerpts e WHERE e.document_id = d.id
                  AND e.relevance != 'out'),
               (SELECT COUNT(*) FROM board_questions q WHERE q.document_id = d.id
                  AND q.relevance = 'out')
             + (SELECT COUNT(*) FROM board_excerpts e WHERE e.document_id = d.id
                  AND e.relevance = 'out'),
               (SELECT COUNT(*) FROM board_questions q WHERE q.document_id = d.id
                  AND q.correct IS NULL)
             FROM board_documents d WHERE d.plan_id = ?1 ORDER BY d.id",
        )?;
        let rows = stmt.query_map([plan.0], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })?;
        rows.map(|row| {
            let (id, kind, file_name, added_on, used, set_aside, unanswered) = row?;
            Ok(BoardDocument {
                id: DocumentId(id),
                plan_id: plan,
                kind: DocumentKind::parse(&kind).ok_or(StoreError::Corrupt {
                    column: "kind",
                    value: kind.clone(),
                })?,
                file_name,
                added_on: parse_date("added_on", &added_on)?,
                used: used as usize,
                set_aside: set_aside as usize,
                unanswered: unanswered as usize,
            })
        })
        .collect()
    }

    pub fn delete_board_document(&mut self, id: DocumentId) -> Result<()> {
        self.conn
            .execute("DELETE FROM board_documents WHERE id = ?1", [id.0])?;
        Ok(())
    }

    /// Original extracted text, retained so another plan can reuse a document.
    pub fn board_document_text(&self, id: DocumentId) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT text FROM board_documents WHERE id = ?1",
                [id.0],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// All questions taken from the plan's documents.
    pub fn board_questions(&self, plan: PlanId) -> Result<Vec<BoardQuestion>> {
        let mut stmt = self.conn.prepare(
            "SELECT q.id, q.document_id, q.number, q.prompt, q.options, q.correct, q.topic_id,
                    q.relevance, q.reason
             FROM board_questions q JOIN board_documents d ON d.id = q.document_id
             WHERE d.plan_id = ?1 ORDER BY q.document_id, q.number, q.id",
        )?;
        let rows = stmt.query_map([plan.0], |r| {
            Ok(BoardQuestion {
                id: BoardQuestionId(r.get(0)?),
                document_id: DocumentId(r.get(1)?),
                number: r.get(2)?,
                prompt: r.get(3)?,
                options: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
                correct: r.get::<_, Option<i64>>(5)?.map(|c| c as usize),
                topic_id: r.get::<_, Option<i64>>(6)?.map(TopicId),
                relevance: Relevance::parse_lenient(&r.get::<_, String>(7)?),
                reason: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// All excerpts taken from the plan's documents.
    pub fn board_excerpts(&self, plan: PlanId) -> Result<Vec<BoardExcerpt>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.document_id, e.summary, e.topic_id, e.relevance, e.reason
             FROM board_excerpts e JOIN board_documents d ON d.id = e.document_id
             WHERE d.plan_id = ?1 ORDER BY e.id",
        )?;
        let rows = stmt.query_map([plan.0], |r| {
            Ok(BoardExcerpt {
                id: ExcerptId(r.get(0)?),
                document_id: DocumentId(r.get(1)?),
                summary: r.get(2)?,
                topic_id: r.get::<_, Option<i64>>(3)?.map(TopicId),
                relevance: Relevance::parse_lenient(&r.get::<_, String>(4)?),
                reason: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn set_question_relevance(
        &mut self,
        id: BoardQuestionId,
        relevance: Relevance,
        topic: Option<TopicId>,
        reason: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE board_questions SET relevance = ?2, topic_id = ?3, reason = ?4 WHERE id = ?1",
            params![id.0, relevance.as_str(), topic.map(|t| t.0), reason],
        )?;
        Ok(())
    }

    pub fn set_excerpt_relevance(
        &mut self,
        id: ExcerptId,
        relevance: Relevance,
        topic: Option<TopicId>,
        reason: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE board_excerpts SET relevance = ?2, topic_id = ?3, reason = ?4 WHERE id = ?1",
            params![id.0, relevance.as_str(), topic.map(|t| t.0), reason],
        )?;
        Ok(())
    }

    /// Sets answers on an exam's questions by question number. Returns how
    /// many questions got an answer.
    pub fn apply_answer_key(
        &mut self,
        exam: DocumentId,
        answers: &[(u32, usize)],
    ) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut updated = 0;
        for (number, index) in answers {
            updated += tx.execute(
                "UPDATE board_questions SET correct = ?3 WHERE document_id = ?1 AND number = ?2",
                params![exam.0, number, *index as i64],
            )?;
        }
        tx.commit()?;
        Ok(updated)
    }

    /// Notes that the student studied today.
    pub fn record_activity(&mut self, kind: ActivityKind, day: NaiveDate) -> Result<()> {
        self.conn.execute(
            "INSERT INTO activity (day, kind) VALUES (?1, ?2)",
            params![day.to_string(), kind.as_str()],
        )?;
        Ok(())
    }

    /// Distinct days with activity, newest first, at most `limit`.
    pub fn activity_days(&self, limit: u32) -> Result<Vec<NaiveDate>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT day FROM activity ORDER BY day DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit], |r| r.get::<_, String>(0))?;
        rows.map(|row| parse_date("day", &row?)).collect()
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
    fn readings_keep_progress_and_highlights() {
        let mut store = Store::open_in_memory().unwrap();
        let plan = store
            .insert_plan(&sample_plan(), "edital.pdf", "texto", today())
            .unwrap();
        let id = store
            .insert_reading(plan.id, "Crase", "C:/apostila.pdf", "{}", today())
            .unwrap();
        let mut reading = store.readings(plan.id).unwrap().remove(0);
        assert_eq!((reading.title.as_str(), reading.page_count), ("Crase", 0));
        assert_eq!(store.reading_document(id).unwrap().as_deref(), Some("{}"));

        reading.page_count = 10;
        reading.last_page = 3;
        reading.pages_seen = [0, 1, 2, 3].into();
        store.save_reading_progress(&reading).unwrap();
        assert_eq!(store.readings(plan.id).unwrap()[0], reading);

        let mark = Highlight {
            id: HighlightId(0),
            page: 2,
            first: 10,
            last: 24,
            text: "antes de palavras femininas".into(),
            color: HighlightColor::Green,
            note: String::new(),
        };
        let mark_id = store.insert_highlight(id, &mark, today()).unwrap();
        store
            .update_highlight(mark_id, HighlightColor::Pink, "cai muito")
            .unwrap();
        let saved = store.highlights(id).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(
            (saved[0].color, saved[0].note.as_str()),
            (HighlightColor::Pink, "cai muito")
        );
        assert_eq!((saved[0].first, saved[0].last), (10, 24));

        store.delete_highlight(mark_id).unwrap();
        assert!(store.highlights(id).unwrap().is_empty());
        store.insert_highlight(id, &mark, today()).unwrap();
        // Deleting the handout takes its highlights with it.
        store.delete_reading(id).unwrap();
        assert!(store.readings(plan.id).unwrap().is_empty());
        assert!(store.highlights(id).unwrap().is_empty());
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
        store
            .record_lesson(topic_id, today(), &LessonRecap::default())
            .unwrap();
        let stats = store.quiz_stats(plan.id).unwrap();
        assert_eq!((stats.attempts, stats.correct, stats.total), (2, 8, 10));
        assert_eq!(store.lesson_count(plan.id).unwrap(), 1);
    }

    #[test]
    fn mistakes_update_on_a_second_miss_and_leave_when_answered_right() {
        let mut store = Store::open_in_memory().unwrap();
        let plan = store
            .insert_plan(&sample_plan(), "e.pdf", "", today())
            .unwrap();
        let topic = plan.subjects[0].topics[0].id;
        let question = QuizQuestion {
            prompt: "Crase antes de palavra masculina?".into(),
            options: vec!["Sim".into(), "Não".into()],
            correct: 1,
            explanation: "Não há artigo feminino.".into(),
            from_board: false,
        };
        store.record_mistake(topic, &question, 0, today()).unwrap();
        store.record_mistake(topic, &question, 0, today()).unwrap();
        let mistakes = store.mistakes(plan.id).unwrap();
        assert_eq!(mistakes.len(), 1);
        assert_eq!((mistakes[0].misses, mistakes[0].chosen), (2, 0));
        assert_eq!(mistakes[0].question, question);
        assert_eq!(store.topic_mistakes(topic).unwrap().len(), 1);

        assert!(store.clear_mistake(topic, &question.prompt).unwrap());
        assert!(store.mistakes(plan.id).unwrap().is_empty());
        assert!(!store.clear_mistake(topic, &question.prompt).unwrap());
    }

    #[test]
    fn lessons_keep_their_recap_and_drafts_their_place() {
        let mut store = Store::open_in_memory().unwrap();
        let plan = store
            .insert_plan(&sample_plan(), "e.pdf", "", today())
            .unwrap();
        let topic = plan.subjects[0].topics[0].id;
        assert_eq!(store.last_lesson(topic).unwrap(), None);
        let recap = LessonRecap {
            taken_on: Some(today()),
            outline: vec!["O que é".into(), "Exceções".into()],
            board: vec!["à + a".into()],
            summary: "Crase é fusão.".into(),
        };
        store.record_lesson(topic, today(), &recap).unwrap();
        assert_eq!(store.last_lesson(topic).unwrap(), Some(recap));

        store.save_lesson_draft(topic, "{}", today()).unwrap();
        store
            .save_lesson_draft(topic, "{\"step\":2}", today())
            .unwrap();
        assert_eq!(
            store.lesson_draft(topic).unwrap().as_deref(),
            Some("{\"step\":2}")
        );
        assert_eq!(store.lesson_draft_topics(plan.id).unwrap(), vec![topic]);
        store.delete_lesson_draft(topic).unwrap();
        assert_eq!(store.lesson_draft(topic).unwrap(), None);
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
    fn handouts_are_cached_per_topic() {
        let mut store = Store::open_in_memory().unwrap();
        let plan = store
            .insert_plan(&sample_plan(), "e.pdf", "", today())
            .unwrap();
        let topic = plan.subjects[0].topics[0].id;
        assert_eq!(store.handout(topic).unwrap(), None);
        store.save_handout(topic, "{\"a\":1}", today()).unwrap();
        store.save_handout(topic, "{\"a\":2}", today()).unwrap();
        assert_eq!(store.handout(topic).unwrap().as_deref(), Some("{\"a\":2}"));
        assert_eq!(store.handout_count(plan.id).unwrap(), 1);
        store.delete_plan(plan.id).unwrap();
        assert_eq!(store.handout(topic).unwrap(), None);
    }

    #[test]
    fn board_material_round_trip() {
        let mut store = Store::open_in_memory().unwrap();
        let plan = store
            .insert_plan(&sample_plan(), "e.pdf", "", today())
            .unwrap();
        let topic = plan.subjects[0].topics[0].id;
        let question = |number, topic_id, relevance| NewBoardQuestion {
            number: Some(number),
            prompt: format!("Questão {number}"),
            options: vec!["Certo".into(), "Errado".into()],
            correct: None,
            topic_id,
            relevance,
            reason: "motivo".into(),
        };
        let doc = store
            .insert_board_document(
                plan.id,
                DocumentKind::Exam,
                "prova.pdf",
                "texto",
                &[
                    question(1, Some(topic), Relevance::Relevant),
                    question(2, None, Relevance::OutOfScope),
                ],
                &[NewBoardExcerpt {
                    summary: "Resumo".into(),
                    topic_id: Some(topic),
                    relevance: Relevance::Partial,
                    reason: String::new(),
                }],
                today(),
            )
            .unwrap();

        let docs = store.board_documents(plan.id).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(
            (docs[0].used, docs[0].set_aside, docs[0].unanswered),
            (2, 1, 2)
        );

        assert_eq!(store.apply_answer_key(doc, &[(1, 1), (7, 0)]).unwrap(), 1);
        let questions = store.board_questions(plan.id).unwrap();
        assert_eq!(questions[0].correct, Some(1));
        assert_eq!(questions[0].options, vec!["Certo", "Errado"]);

        store
            .set_question_relevance(questions[1].id, Relevance::Relevant, Some(topic), "manual")
            .unwrap();
        assert_eq!(store.board_documents(plan.id).unwrap()[0].set_aside, 0);
        assert_eq!(store.board_excerpts(plan.id).unwrap().len(), 1);

        store.delete_board_document(doc).unwrap();
        assert!(store.board_questions(plan.id).unwrap().is_empty());
    }

    #[test]
    fn activity_days_are_distinct_and_newest_first() {
        let mut store = Store::open_in_memory().unwrap();
        let day = |d| NaiveDate::from_ymd_opt(2026, 5, d).unwrap();
        store.record_activity(ActivityKind::Lesson, day(2)).unwrap();
        store.record_activity(ActivityKind::Quiz, day(3)).unwrap();
        store.record_activity(ActivityKind::Review, day(3)).unwrap();
        assert_eq!(store.activity_days(10).unwrap(), vec![day(3), day(2)]);
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
