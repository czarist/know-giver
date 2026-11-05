//! Storage of mock exams: the plan's exam format, the exams and their
//! timed attempts.

use chrono::NaiveDateTime;
use rusqlite::{OptionalExtension, Row, params};

use super::discursive::{from_json, json, parse_time, time};
use super::{Result, Store};
use crate::discursive::ProposalId;
use crate::mock::{
    ExamFormat, MockAttempt, MockAttemptId, MockExam, MockExamId, MockQuestion, NewMockExam,
};
use crate::plan::PlanId;

impl Store {
    /// The exam format saved for a plan, if any.
    pub fn exam_format(&self, plan: PlanId) -> Result<Option<ExamFormat>> {
        let text: Option<String> = self
            .conn
            .query_row(
                "SELECT format FROM exam_formats WHERE plan_id = ?1",
                [plan.0],
                |r| r.get(0),
            )
            .optional()?;
        text.map(|t| from_json("exam_formats.format", &t))
            .transpose()
    }

    pub fn save_exam_format(&mut self, plan: PlanId, format: &ExamFormat) -> Result<()> {
        self.conn.execute(
            "INSERT INTO exam_formats (plan_id, format) VALUES (?1, ?2)
             ON CONFLICT(plan_id) DO UPDATE SET format = excluded.format",
            params![plan.0, json(format)?],
        )?;
        Ok(())
    }

    pub fn insert_mock_exam(
        &mut self,
        plan: PlanId,
        exam: &NewMockExam,
        created_at: NaiveDateTime,
    ) -> Result<MockExamId> {
        self.conn.execute(
            "INSERT INTO mock_exams
               (plan_id, title, format, questions, proposal_id, model, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                plan.0,
                exam.title,
                json(&exam.format)?,
                json(&exam.questions)?,
                exam.proposal.map(|p| p.0),
                exam.model,
                time(created_at)
            ],
        )?;
        Ok(MockExamId(self.conn.last_insert_rowid()))
    }

    /// Mock exams of a plan, newest first.
    pub fn mock_exams(&self, plan: PlanId) -> Result<Vec<MockExam>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, plan_id, title, format, questions, proposal_id, model, created_at
             FROM mock_exams WHERE plan_id = ?1 ORDER BY created_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([plan.0], raw_exam)?;
        rows.map(|row| row.map_err(Into::into).and_then(exam_from_raw))
            .collect()
    }

    pub fn delete_mock_exam(&mut self, id: MockExamId) -> Result<()> {
        self.conn
            .execute("DELETE FROM mock_exams WHERE id = ?1", [id.0])?;
        Ok(())
    }

    /// Starts the clock on a new attempt with every answer blank.
    pub fn start_mock_attempt(
        &mut self,
        exam: MockExamId,
        questions: usize,
        started_at: NaiveDateTime,
    ) -> Result<MockAttempt> {
        let answers: Vec<Option<u8>> = vec![None; questions];
        self.conn.execute(
            "INSERT INTO mock_attempts (exam_id, started_at, answers) VALUES (?1, ?2, ?3)",
            params![exam.0, time(started_at), json(&answers)?],
        )?;
        Ok(MockAttempt {
            id: MockAttemptId(self.conn.last_insert_rowid()),
            exam_id: exam,
            started_at,
            finished_at: None,
            answers,
        })
    }

    pub fn save_mock_answers(&mut self, id: MockAttemptId, answers: &[Option<u8>]) -> Result<()> {
        self.conn.execute(
            "UPDATE mock_attempts SET answers = ?2 WHERE id = ?1",
            params![id.0, json(&answers)?],
        )?;
        Ok(())
    }

    pub fn finish_mock_attempt(
        &mut self,
        id: MockAttemptId,
        answers: &[Option<u8>],
        finished_at: NaiveDateTime,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE mock_attempts SET answers = ?2, finished_at = ?3 WHERE id = ?1",
            params![id.0, json(&answers)?, time(finished_at)],
        )?;
        Ok(())
    }

    pub fn delete_mock_attempt(&mut self, id: MockAttemptId) -> Result<()> {
        self.conn
            .execute("DELETE FROM mock_attempts WHERE id = ?1", [id.0])?;
        Ok(())
    }

    /// Attempts on the mock exams of a plan, newest first.
    pub fn mock_attempts(&self, plan: PlanId) -> Result<Vec<MockAttempt>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.id, a.exam_id, a.started_at, a.finished_at, a.answers
             FROM mock_attempts a JOIN mock_exams e ON e.id = a.exam_id
             WHERE e.plan_id = ?1 ORDER BY a.started_at DESC, a.id DESC",
        )?;
        let rows = stmt.query_map([plan.0], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        rows.map(|row| {
            let (id, exam, started, finished, answers) = row?;
            Ok(MockAttempt {
                id: MockAttemptId(id),
                exam_id: MockExamId(exam),
                started_at: parse_time("mock_attempts.started_at", &started)?,
                finished_at: finished
                    .map(|f| parse_time("mock_attempts.finished_at", &f))
                    .transpose()?,
                answers: from_json("mock_attempts.answers", &answers)?,
            })
        })
        .collect()
    }
}

type RawExam = (
    i64,
    i64,
    String,
    String,
    String,
    Option<i64>,
    String,
    String,
);

fn raw_exam(r: &Row<'_>) -> rusqlite::Result<RawExam> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
    ))
}

fn exam_from_raw(raw: RawExam) -> Result<MockExam> {
    let (id, plan, title, format, questions, proposal, model, created) = raw;
    Ok(MockExam {
        id: MockExamId(id),
        plan_id: PlanId(plan),
        title,
        format: from_json("mock_exams.format", &format)?,
        questions: from_json::<Vec<MockQuestion>>("mock_exams.questions", &questions)?,
        proposal: proposal.map(ProposalId),
        model,
        created_at: parse_time("mock_exams.created_at", &created)?,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveDateTime};

    use crate::Store;
    use crate::mock::{
        AnswerStyle, ExamFormat, ExamSection, FormatSource, MockQuestion, NewMockExam,
    };
    use crate::plan::{Difficulty, NewPlan, NewSubject, NewTopic, PlanId, TopicId};

    fn at(hour: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 5)
            .and_then(|d| d.and_hms_opt(hour, 0, 0))
            .unwrap()
    }

    fn format() -> ExamFormat {
        ExamFormat {
            minutes: 60,
            style: AnswerStyle::Choice { options: 4 },
            wrong_cancels: false,
            sections: vec![ExamSection {
                subject: "Português".into(),
                questions: 2,
                weight: 1.0,
            }],
            essay: false,
            source: FormatSource::Notice,
        }
    }

    fn store_with_plan() -> (Store, PlanId, TopicId) {
        let mut store = Store::open_in_memory().unwrap();
        let new = NewPlan::new(
            "Plano",
            "Concurso",
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
            .insert_plan(
                &new,
                "edital.pdf",
                "texto",
                NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            )
            .unwrap();
        let topic = plan.subjects[0].topics[0].id;
        (store, plan.id, topic)
    }

    #[test]
    fn format_is_saved_per_plan_and_replaced() {
        let (mut store, plan, _) = store_with_plan();
        assert_eq!(store.exam_format(plan).unwrap(), None);
        let mut f = format();
        store.save_exam_format(plan, &f).unwrap();
        f.minutes = 90;
        f.source = FormatSource::Student;
        store.save_exam_format(plan, &f).unwrap();
        assert_eq!(store.exam_format(plan).unwrap(), Some(f));
    }

    #[test]
    fn exam_and_attempt_round_trip_and_follow_the_plan() {
        let (mut store, plan, topic) = store_with_plan();
        let question = |correct: u8| MockQuestion {
            section: 0,
            topic: Some(topic),
            prompt: format!("Pergunta {correct}"),
            options: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            correct,
            explanation: "porque".into(),
        };
        let new = NewMockExam {
            title: "Simulado 1".into(),
            format: format(),
            questions: vec![question(0), question(3)],
            proposal: None,
            model: "m".into(),
        };
        let id = store.insert_mock_exam(plan, &new, at(9)).unwrap();
        let exams = store.mock_exams(plan).unwrap();
        assert_eq!(exams.len(), 1);
        assert_eq!(exams[0].questions, new.questions);

        let attempt = store.start_mock_attempt(id, 2, at(10)).unwrap();
        store
            .save_mock_answers(attempt.id, &[Some(1), None])
            .unwrap();
        let saved = &store.mock_attempts(plan).unwrap()[0];
        assert_eq!(saved.answers, vec![Some(1), None]);
        assert_eq!(saved.finished_at, None);

        store
            .finish_mock_attempt(attempt.id, &[Some(0), Some(3)], at(11))
            .unwrap();
        let saved = &store.mock_attempts(plan).unwrap()[0];
        assert_eq!(saved.finished_at, Some(at(11)));
        assert_eq!(saved.started_at, at(10));

        store.delete_plan(plan).unwrap();
        assert!(store.mock_exams(plan).unwrap().is_empty());
        assert!(store.mock_attempts(plan).unwrap().is_empty());
    }
}
