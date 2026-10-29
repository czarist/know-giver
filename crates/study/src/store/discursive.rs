//! Storage of written answers: proposals, attempts, pages, transcripts,
//! evaluations and the recurring-difficulties report.

use chrono::NaiveDateTime;
use rusqlite::{OptionalExtension, Row, params};

use super::{Result, Store, StoreError};
use crate::discursive::{
    Attempt, AttemptId, AttemptOrigin, AttemptStatus, ConfirmedTranscript, DifficultyReport,
    Evaluation, NewPage, NewProposal, Page, PageId, PageIssue, Proposal, ProposalId,
    RecurringIssue, Rubric, Stage, Transcript, TranscriptRecord, WritingKind,
};
use crate::plan::{Difficulty, PlanId, TopicId};

const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

fn time(value: NaiveDateTime) -> String {
    value.format(TIME_FORMAT).to_string()
}

fn parse_time(column: &'static str, value: &str) -> Result<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, TIME_FORMAT).map_err(|_| StoreError::Corrupt {
        column,
        value: value.to_owned(),
    })
}

fn json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| StoreError::Corrupt {
        column: "json",
        value: e.to_string(),
    })
}

fn from_json<T: serde::de::DeserializeOwned>(column: &'static str, value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|_| StoreError::Corrupt {
        column,
        value: value.chars().take(80).collect(),
    })
}

fn corrupt(column: &'static str, value: &str) -> StoreError {
    StoreError::Corrupt {
        column,
        value: value.to_owned(),
    }
}

const PROPOSAL_COLUMNS: &str = "id, plan_id, kind, difficulty, theme, statement, instructions,
    topics, line_limit, minutes, rubric, expected, model, prompt_version, created_at";

type ProposalRow = (
    i64,
    i64,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<i64>,
    Option<i64>,
    String,
    String,
    String,
    String,
    String,
);

fn proposal_row(r: &Row<'_>) -> rusqlite::Result<ProposalRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
        r.get(8)?,
        r.get(9)?,
        r.get(10)?,
        r.get(11)?,
        r.get(12)?,
        r.get(13)?,
        r.get(14)?,
    ))
}

fn proposal_from(row: ProposalRow) -> Result<Proposal> {
    let (
        id,
        plan,
        kind,
        difficulty,
        theme,
        statement,
        instructions,
        topics,
        line_limit,
        minutes,
        rubric,
        expected,
        model,
        prompt_version,
        created_at,
    ) = row;
    let topics: Vec<i64> = from_json("topics", &topics)?;
    Ok(Proposal {
        id: ProposalId(id),
        plan_id: PlanId(plan),
        kind: WritingKind::parse(&kind).ok_or_else(|| corrupt("kind", &kind))?,
        difficulty: Difficulty::parse_lenient(&difficulty),
        theme,
        statement,
        instructions,
        topics: topics.into_iter().map(TopicId).collect(),
        line_limit: line_limit.and_then(|v| u32::try_from(v).ok()),
        minutes: minutes.and_then(|v| u32::try_from(v).ok()),
        rubric: from_json::<Rubric>("rubric", &rubric)?,
        expected_points: from_json("expected", &expected)?,
        model,
        prompt_version,
        created_at: parse_time("created_at", &created_at)?,
    })
}

const ATTEMPT_COLUMNS: &str = "a.id, a.proposal_id, a.number, a.parent_id, a.origin, a.status,
    a.failed_stage, a.note, a.run, a.created_at, a.updated_at, e.total, e.max_total";

type AttemptRow = (
    i64,
    i64,
    i64,
    Option<i64>,
    String,
    String,
    Option<String>,
    String,
    i64,
    String,
    String,
    Option<f64>,
    Option<f64>,
);

fn attempt_row(r: &Row<'_>) -> rusqlite::Result<AttemptRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
        r.get(8)?,
        r.get(9)?,
        r.get(10)?,
        r.get(11)?,
        r.get(12)?,
    ))
}

fn attempt_from(row: AttemptRow) -> Result<Attempt> {
    let (
        id,
        proposal,
        number,
        parent,
        origin,
        status,
        stage,
        note,
        run,
        created,
        updated,
        total,
        max,
    ) = row;
    Ok(Attempt {
        id: AttemptId(id),
        proposal_id: ProposalId(proposal),
        number: u32::try_from(number).unwrap_or(0),
        parent: parent.map(AttemptId),
        origin: AttemptOrigin::parse(&origin).ok_or_else(|| corrupt("origin", &origin))?,
        status: AttemptStatus::parse(&status).ok_or_else(|| corrupt("status", &status))?,
        failed_stage: stage.as_deref().and_then(Stage::parse),
        note,
        run: u32::try_from(run).unwrap_or(0),
        created_at: parse_time("created_at", &created)?,
        updated_at: parse_time("updated_at", &updated)?,
        score: total.zip(max),
    })
}

impl Store {
    pub fn insert_proposal(
        &mut self,
        plan: PlanId,
        proposal: &NewProposal,
        now: NaiveDateTime,
    ) -> Result<ProposalId> {
        let topics: Vec<i64> = proposal.topics.iter().map(|t| t.0).collect();
        self.conn.execute(
            "INSERT INTO discursive_proposals (plan_id, kind, difficulty, theme, statement,
               instructions, topics, line_limit, minutes, rubric, expected, model,
               prompt_version, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                plan.0,
                proposal.kind.as_str(),
                proposal.difficulty.as_str(),
                proposal.theme,
                proposal.statement,
                proposal.instructions,
                json(&topics)?,
                proposal.line_limit,
                proposal.minutes,
                json(&proposal.rubric)?,
                json(&proposal.expected_points)?,
                proposal.model,
                proposal.prompt_version,
                time(now),
            ],
        )?;
        Ok(ProposalId(self.conn.last_insert_rowid()))
    }

    /// The plan's proposals, newest first.
    pub fn proposals(&self, plan: PlanId) -> Result<Vec<Proposal>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {PROPOSAL_COLUMNS} FROM discursive_proposals WHERE plan_id = ?1
             ORDER BY id DESC"
        ))?;
        let rows = stmt.query_map([plan.0], proposal_row)?;
        rows.map(|row| proposal_from(row?)).collect()
    }

    pub fn proposal(&self, id: ProposalId) -> Result<Option<Proposal>> {
        let row = self
            .conn
            .query_row(
                &format!("SELECT {PROPOSAL_COLUMNS} FROM discursive_proposals WHERE id = ?1"),
                [id.0],
                proposal_row,
            )
            .optional()?;
        row.map(proposal_from).transpose()
    }

    /// Deletes a proposal with its attempts. Returns the pages' files, for
    /// the caller to remove.
    pub fn delete_proposal(&mut self, id: ProposalId) -> Result<Vec<AttemptId>> {
        let attempts: Vec<AttemptId> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM discursive_attempts WHERE proposal_id = ?1")?;
            let rows = stmt.query_map([id.0], |r| r.get::<_, i64>(0))?;
            rows.map(|r| r.map(AttemptId))
                .collect::<rusqlite::Result<_>>()?
        };
        self.conn
            .execute("DELETE FROM discursive_proposals WHERE id = ?1", [id.0])?;
        Ok(attempts)
    }

    /// A new attempt on a proposal, numbered after the existing ones.
    pub fn insert_attempt(
        &mut self,
        proposal: ProposalId,
        origin: AttemptOrigin,
        parent: Option<AttemptId>,
        status: AttemptStatus,
        now: NaiveDateTime,
    ) -> Result<AttemptId> {
        let tx = self.conn.transaction()?;
        let number: i64 = tx.query_row(
            "SELECT COALESCE(MAX(number), 0) + 1 FROM discursive_attempts WHERE proposal_id = ?1",
            [proposal.0],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO discursive_attempts (proposal_id, number, parent_id, origin, status,
               created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![
                proposal.0,
                number,
                parent.map(|p| p.0),
                origin.as_str(),
                status.as_str(),
                time(now)
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(AttemptId(id))
    }

    /// Every attempt of the plan's proposals, oldest first.
    pub fn attempts(&self, plan: PlanId) -> Result<Vec<Attempt>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {ATTEMPT_COLUMNS} FROM discursive_attempts a
             JOIN discursive_proposals p ON p.id = a.proposal_id
             LEFT JOIN discursive_evaluations e ON e.attempt_id = a.id
             WHERE p.plan_id = ?1 ORDER BY a.id"
        ))?;
        let rows = stmt.query_map([plan.0], attempt_row)?;
        rows.map(|row| attempt_from(row?)).collect()
    }

    pub fn attempt(&self, id: AttemptId) -> Result<Option<Attempt>> {
        let row = self
            .conn
            .query_row(
                &format!(
                    "SELECT {ATTEMPT_COLUMNS} FROM discursive_attempts a
                     LEFT JOIN discursive_evaluations e ON e.attempt_id = a.id
                     WHERE a.id = ?1"
                ),
                [id.0],
                attempt_row,
            )
            .optional()?;
        row.map(attempt_from).transpose()
    }

    /// Moves an attempt to `status`, with the failed stage and a note.
    pub fn set_attempt_status(
        &mut self,
        id: AttemptId,
        status: AttemptStatus,
        failed_stage: Option<Stage>,
        note: &str,
        now: NaiveDateTime,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE discursive_attempts SET status = ?2, failed_stage = ?3, note = ?4,
               updated_at = ?5 WHERE id = ?1",
            params![
                id.0,
                status.as_str(),
                failed_stage.map(Stage::as_str),
                note,
                time(now)
            ],
        )?;
        Ok(())
    }

    /// Starts background work on an attempt: sets `status` and returns the
    /// new run number. `None` when the attempt is gone or already running,
    /// so repeated clicks start nothing twice.
    pub fn begin_run(
        &mut self,
        id: AttemptId,
        status: AttemptStatus,
        now: NaiveDateTime,
    ) -> Result<Option<u32>> {
        let changed = self.conn.execute(
            "UPDATE discursive_attempts SET status = ?2, run = run + 1, failed_stage = NULL,
               note = '', updated_at = ?3
             WHERE id = ?1 AND status NOT IN ('transcribing', 'evaluating')",
            params![id.0, status.as_str(), time(now)],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        let run: i64 = self.conn.query_row(
            "SELECT run FROM discursive_attempts WHERE id = ?1",
            [id.0],
            |r| r.get(0),
        )?;
        Ok(u32::try_from(run).ok())
    }

    /// Attempts left processing when the app closed are marked failed, to
    /// be run again. Returns how many.
    pub fn fail_interrupted_attempts(&mut self, note: &str, now: NaiveDateTime) -> Result<usize> {
        let mut changed = 0;
        for (status, stage) in [
            (AttemptStatus::Transcribing, Stage::Transcription),
            (AttemptStatus::Evaluating, Stage::Evaluation),
        ] {
            changed += self.conn.execute(
                "UPDATE discursive_attempts SET status = 'failed', failed_stage = ?2,
                   note = ?3, updated_at = ?4 WHERE status = ?1",
                params![status.as_str(), stage.as_str(), note, time(now)],
            )?;
        }
        Ok(changed)
    }

    pub fn delete_attempt(&mut self, id: AttemptId) -> Result<()> {
        self.conn
            .execute("DELETE FROM discursive_attempts WHERE id = ?1", [id.0])?;
        Ok(())
    }

    /// Attempts of every proposal of a plan, to remove their files before
    /// the plan is deleted.
    pub fn plan_attempt_ids(&self, plan: PlanId) -> Result<Vec<AttemptId>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.id FROM discursive_attempts a
             JOIN discursive_proposals p ON p.id = a.proposal_id WHERE p.plan_id = ?1",
        )?;
        let rows = stmt.query_map([plan.0], |r| r.get::<_, i64>(0))?;
        Ok(rows
            .map(|r| r.map(AttemptId))
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Adds a page after the existing ones.
    pub fn add_page(&mut self, attempt: AttemptId, page: &NewPage) -> Result<PageId> {
        let tx = self.conn.transaction()?;
        let position: i64 = tx.query_row(
            "SELECT COUNT(*) FROM discursive_pages WHERE attempt_id = ?1",
            [attempt.0],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO discursive_pages (attempt_id, position, original_path, processed_path,
               rotation, width, height, issues)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                attempt.0,
                position,
                page.original_path,
                page.processed_path,
                page.rotation,
                page.width,
                page.height,
                json(&page.issues)?
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(PageId(id))
    }

    /// An attempt's pages, in order.
    pub fn pages(&self, attempt: AttemptId) -> Result<Vec<Page>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, position, original_path, processed_path, rotation, width, height, issues
             FROM discursive_pages WHERE attempt_id = ?1 ORDER BY position, id",
        )?;
        let rows = stmt.query_map([attempt.0], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, String>(7)?,
            ))
        })?;
        rows.map(|row| {
            let (id, position, original_path, processed_path, rotation, width, height, issues) =
                row?;
            Ok(Page {
                id: PageId(id),
                attempt_id: attempt,
                position: u32::try_from(position).unwrap_or(0),
                original_path,
                processed_path,
                rotation: u8::try_from(rotation.rem_euclid(4)).unwrap_or(0),
                width: u32::try_from(width).unwrap_or(0),
                height: u32::try_from(height).unwrap_or(0),
                issues: from_json::<Vec<PageIssue>>("issues", &issues)?,
            })
        })
        .collect()
    }

    /// Replaces a page's files, size, rotation and issues, keeping its place.
    pub fn update_page(&mut self, id: PageId, page: &NewPage) -> Result<()> {
        self.conn.execute(
            "UPDATE discursive_pages SET original_path = ?2, processed_path = ?3, rotation = ?4,
               width = ?5, height = ?6, issues = ?7 WHERE id = ?1",
            params![
                id.0,
                page.original_path,
                page.processed_path,
                page.rotation,
                page.width,
                page.height,
                json(&page.issues)?
            ],
        )?;
        Ok(())
    }

    /// Moves a page one place earlier (`up`) or later. Returns false when it
    /// is already first or last.
    pub fn move_page(&mut self, id: PageId, up: bool) -> Result<bool> {
        let tx = self.conn.transaction()?;
        let Some((attempt, position)) = tx
            .query_row(
                "SELECT attempt_id, position FROM discursive_pages WHERE id = ?1",
                [id.0],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?
        else {
            return Ok(false);
        };
        let other = if up { position - 1 } else { position + 1 };
        let changed = tx.execute(
            "UPDATE discursive_pages SET position = ?3 WHERE attempt_id = ?1 AND position = ?2",
            params![attempt, other, position],
        )?;
        if changed == 0 {
            return Ok(false);
        }
        tx.execute(
            "UPDATE discursive_pages SET position = ?2 WHERE id = ?1",
            params![id.0, other],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Removes a page and closes the gap. Returns its paths, for the caller
    /// to remove the files.
    pub fn remove_page(&mut self, id: PageId) -> Result<Option<(String, String)>> {
        let tx = self.conn.transaction()?;
        let Some((attempt, position, original, processed)) = tx
            .query_row(
                "SELECT attempt_id, position, original_path, processed_path
                 FROM discursive_pages WHERE id = ?1",
                [id.0],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?
        else {
            return Ok(None);
        };
        tx.execute("DELETE FROM discursive_pages WHERE id = ?1", [id.0])?;
        tx.execute(
            "UPDATE discursive_pages SET position = position - 1
             WHERE attempt_id = ?1 AND position > ?2",
            params![attempt, position],
        )?;
        tx.commit()?;
        Ok(Some((original, processed)))
    }

    /// Saves what the model read. A new reading (after new photos) replaces
    /// the previous one and its confirmation.
    pub fn save_transcript(
        &mut self,
        attempt: AttemptId,
        transcript: &Transcript,
        model: &str,
        prompt_version: &str,
        now: NaiveDateTime,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO discursive_transcripts
               (attempt_id, original, confirmed, model, prompt_version, created_at, confirmed_at)
             VALUES (?1, ?2, NULL, ?3, ?4, ?5, NULL)",
            params![
                attempt.0,
                json(transcript)?,
                model,
                prompt_version,
                time(now)
            ],
        )?;
        Ok(())
    }

    /// Saves the student's checked version, leaving the model's untouched.
    pub fn confirm_transcript(
        &mut self,
        attempt: AttemptId,
        confirmed: &ConfirmedTranscript,
        now: NaiveDateTime,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE discursive_transcripts SET confirmed = ?2, confirmed_at = ?3
             WHERE attempt_id = ?1",
            params![attempt.0, json(confirmed)?, time(now)],
        )?;
        Ok(())
    }

    pub fn transcript(&self, attempt: AttemptId) -> Result<Option<TranscriptRecord>> {
        let row = self
            .conn
            .query_row(
                "SELECT original, confirmed, model, prompt_version, created_at, confirmed_at
                 FROM discursive_transcripts WHERE attempt_id = ?1",
                [attempt.0],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((original, confirmed, model, prompt_version, created, confirmed_at)) = row else {
            return Ok(None);
        };
        Ok(Some(TranscriptRecord {
            original: from_json("original", &original)?,
            confirmed: confirmed
                .map(|c| from_json::<ConfirmedTranscript>("confirmed", &c))
                .transpose()?,
            model,
            prompt_version,
            created_at: parse_time("created_at", &created)?,
            confirmed_at: confirmed_at
                .map(|c| parse_time("confirmed_at", &c))
                .transpose()?,
        }))
    }

    /// Saves an attempt's evaluation. An attempt has at most one; a second
    /// one is refused (returns false).
    pub fn save_evaluation(&mut self, attempt: AttemptId, evaluation: &Evaluation) -> Result<bool> {
        let changed = self.conn.execute(
            "INSERT OR IGNORE INTO discursive_evaluations
               (attempt_id, content, rubric_key, total, max_total, model, prompt_version,
                evaluated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                attempt.0,
                json(evaluation)?,
                evaluation.rubric.key(),
                evaluation.total(),
                evaluation.max_total(),
                evaluation.model,
                evaluation.prompt_version,
                time(evaluation.evaluated_at)
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn evaluation(&self, attempt: AttemptId) -> Result<Option<Evaluation>> {
        let row: Option<String> = self
            .conn
            .query_row(
                "SELECT content FROM discursive_evaluations WHERE attempt_id = ?1",
                [attempt.0],
                |r| r.get(0),
            )
            .optional()?;
        row.map(|c| from_json("content", &c)).transpose()
    }

    /// Every evaluation of the plan, oldest first.
    pub fn evaluations(&self, plan: PlanId) -> Result<Vec<(AttemptId, Evaluation)>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.attempt_id, e.content FROM discursive_evaluations e
             JOIN discursive_attempts a ON a.id = e.attempt_id
             JOIN discursive_proposals p ON p.id = a.proposal_id
             WHERE p.plan_id = ?1 ORDER BY e.evaluated_at, e.attempt_id",
        )?;
        let rows = stmt.query_map([plan.0], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (id, content) = row?;
            Ok((AttemptId(id), from_json("content", &content)?))
        })
        .collect()
    }

    pub fn save_difficulty_report(
        &mut self,
        plan: PlanId,
        report: &DifficultyReport,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO discursive_difficulties
               (plan_id, issues, attempts, model, prompt_version, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                plan.0,
                json(&report.issues)?,
                json(&report.attempts)?,
                report.model,
                report.prompt_version,
                time(report.created_at)
            ],
        )?;
        Ok(())
    }

    pub fn difficulty_report(&self, plan: PlanId) -> Result<Option<DifficultyReport>> {
        let row = self
            .conn
            .query_row(
                "SELECT issues, attempts, model, prompt_version, created_at
                 FROM discursive_difficulties WHERE plan_id = ?1",
                [plan.0],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((issues, attempts, model, prompt_version, created)) = row else {
            return Ok(None);
        };
        Ok(Some(DifficultyReport {
            issues: from_json::<Vec<RecurringIssue>>("issues", &issues)?,
            attempts: from_json("attempts", &attempts)?,
            model,
            prompt_version,
            created_at: parse_time("created_at", &created)?,
        }))
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::discursive::{
        CriterionScore, Legibility, TranscriptPage, generic_rubric, parse_manual_rubric,
    };
    use crate::plan::{NewPlan, NewSubject, NewTopic};

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 29)
            .and_then(|d| d.and_hms_opt(10, 30, 0))
            .unwrap()
    }

    fn store_with_plan() -> (Store, PlanId, TopicId) {
        let mut store = Store::open_in_memory().unwrap();
        let plan = NewPlan::new(
            "Plano",
            "TRF",
            4,
            vec![NewSubject {
                name: "Português".into(),
                topics: vec![NewTopic {
                    title: "Crase".into(),
                    difficulty: Difficulty::Medium,
                    week: 1,
                }],
            }],
        )
        .unwrap();
        let plan = store
            .insert_plan(&plan, "edital.pdf", "texto", now().date())
            .unwrap();
        let topic = plan.subjects[0].topics[0].id;
        (store, plan.id, topic)
    }

    fn proposal(topic: TopicId) -> NewProposal {
        NewProposal {
            kind: WritingKind::Technical,
            difficulty: Difficulty::Hard,
            theme: "Crase".into(),
            statement: "Explique o uso da crase.".into(),
            instructions: "Até 20 linhas.".into(),
            topics: vec![topic],
            line_limit: Some(20),
            minutes: None,
            rubric: generic_rubric(WritingKind::Technical),
            expected_points: vec!["fusão da preposição com o artigo".into()],
            model: "m".into(),
            prompt_version: "p".into(),
        }
    }

    fn new_page(name: &str) -> NewPage {
        NewPage {
            original_path: format!("{name}.jpg"),
            processed_path: format!("{name}-p.jpg"),
            rotation: 0,
            width: 1200,
            height: 1600,
            issues: vec![PageIssue::Blurry],
        }
    }

    #[test]
    fn proposals_keep_their_rubric_version() {
        let (mut store, plan, topic) = store_with_plan();
        let mut new = proposal(topic);
        new.rubric = parse_manual_rubric("Conteúdo: 7\nLíngua: 3").unwrap();
        let id = store.insert_proposal(plan, &new, now()).unwrap();
        let saved = store.proposal(id).unwrap().unwrap();
        assert_eq!(saved.rubric, new.rubric);
        assert_eq!(saved.topics, vec![topic]);
        assert_eq!(saved.line_limit, Some(20));
        assert_eq!(saved.created_at, now());
        assert_eq!(store.proposals(plan).unwrap().len(), 1);
    }

    #[test]
    fn pages_keep_order_through_moves_and_removals() {
        let (mut store, plan, topic) = store_with_plan();
        let proposal = store
            .insert_proposal(plan, &proposal(topic), now())
            .unwrap();
        let attempt = store
            .insert_attempt(
                proposal,
                AttemptOrigin::Photos,
                None,
                AttemptStatus::Draft,
                now(),
            )
            .unwrap();
        let a = store.add_page(attempt, &new_page("a")).unwrap();
        let b = store.add_page(attempt, &new_page("b")).unwrap();
        let c = store.add_page(attempt, &new_page("c")).unwrap();
        let order = |store: &Store| -> Vec<PageId> {
            store.pages(attempt).unwrap().iter().map(|p| p.id).collect()
        };
        assert_eq!(order(&store), vec![a, b, c]);
        assert!(store.move_page(c, true).unwrap());
        assert_eq!(order(&store), vec![a, c, b]);
        assert!(!store.move_page(a, true).unwrap(), "already first");
        assert!(!store.move_page(b, false).unwrap(), "already last");
        let removed = store.remove_page(a).unwrap().unwrap();
        assert_eq!(removed.0, "a.jpg");
        assert_eq!(order(&store), vec![c, b]);
        let positions: Vec<u32> = store
            .pages(attempt)
            .unwrap()
            .iter()
            .map(|p| p.position)
            .collect();
        assert_eq!(positions, vec![0, 1]);
        assert_eq!(
            store.pages(attempt).unwrap()[0].issues,
            vec![PageIssue::Blurry]
        );
    }

    #[test]
    fn a_run_starts_once_and_interrupted_runs_fail() {
        let (mut store, plan, topic) = store_with_plan();
        let proposal = store
            .insert_proposal(plan, &proposal(topic), now())
            .unwrap();
        let attempt = store
            .insert_attempt(
                proposal,
                AttemptOrigin::Photos,
                None,
                AttemptStatus::Draft,
                now(),
            )
            .unwrap();
        let run = store
            .begin_run(attempt, AttemptStatus::Transcribing, now())
            .unwrap();
        assert_eq!(run, Some(1));
        // A second click while it runs starts nothing.
        assert_eq!(
            store
                .begin_run(attempt, AttemptStatus::Transcribing, now())
                .unwrap(),
            None
        );
        assert_eq!(store.fail_interrupted_attempts("closed", now()).unwrap(), 1);
        let saved = store.attempt(attempt).unwrap().unwrap();
        assert_eq!(saved.status, AttemptStatus::Failed);
        assert_eq!(saved.failed_stage, Some(Stage::Transcription));
        assert_eq!(
            store
                .begin_run(attempt, AttemptStatus::Transcribing, now())
                .unwrap(),
            Some(2)
        );
    }

    #[test]
    fn transcripts_keep_the_original_and_evaluations_are_not_duplicated() {
        let (mut store, plan, topic) = store_with_plan();
        let proposal_id = store
            .insert_proposal(plan, &proposal(topic), now())
            .unwrap();
        let first = store
            .insert_attempt(
                proposal_id,
                AttemptOrigin::Photos,
                None,
                AttemptStatus::Draft,
                now(),
            )
            .unwrap();
        let second = store
            .insert_attempt(
                proposal_id,
                AttemptOrigin::Rewrite,
                Some(first),
                AttemptStatus::Review,
                now(),
            )
            .unwrap();
        assert_eq!(store.attempt(second).unwrap().unwrap().number, 2);
        let original = Transcript {
            pages: vec![TranscriptPage {
                page: 1,
                lines: vec!["A crase ocore.".into()],
                uncertain: Vec::new(),
                legibility: Legibility::Good,
                warnings: Vec::new(),
                retake: None,
            }],
        };
        store
            .save_transcript(first, &original, "vision", "t1", now())
            .unwrap();
        let confirmed = ConfirmedTranscript {
            pages: vec!["A crase ocorre.".into()],
            edits: crate::discursive::edits(&original, &["A crase ocorre.".into()]),
        };
        store.confirm_transcript(first, &confirmed, now()).unwrap();
        let saved = store.transcript(first).unwrap().unwrap();
        assert_eq!(saved.original, original);
        assert_eq!(saved.confirmed.as_ref().unwrap().edits.len(), 1);
        assert!(
            store.transcript(second).unwrap().is_none(),
            "attempts are apart"
        );

        let rubric = generic_rubric(WritingKind::Technical);
        let evaluation = Evaluation {
            scores: rubric
                .criteria
                .iter()
                .map(|c| CriterionScore {
                    key: c.key.clone(),
                    score: c.max_points,
                    justification: String::new(),
                    evidence: Vec::new(),
                })
                .collect(),
            rubric,
            strengths: Vec::new(),
            problems: Vec::new(),
            priorities: Vec::new(),
            review_topics: vec![topic],
            dropped_quotes: 0,
            model: "m".into(),
            prompt_version: "e1".into(),
            evaluated_at: now(),
        };
        assert!(store.save_evaluation(first, &evaluation).unwrap());
        assert!(!store.save_evaluation(first, &evaluation).unwrap());
        assert_eq!(
            store.attempt(first).unwrap().unwrap().score,
            Some((100.0, 100.0))
        );
        assert_eq!(store.attempt(second).unwrap().unwrap().score, None);
        assert_eq!(store.evaluations(plan).unwrap().len(), 1);

        assert_eq!(store.delete_proposal(proposal_id).unwrap().len(), 2);
        assert!(store.attempt(first).unwrap().is_none());
        assert!(store.transcript(first).unwrap().is_none());
    }
}
