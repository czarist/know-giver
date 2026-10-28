//! Manual check of board material against the real AI endpoint in `.env`:
//! plan from an edital, then a past exam and its answer key, then a quiz.
//!
//! cargo run -p kg-engine --example board_check -- <edital.pdf> <exam.pdf> [answer-key.pdf]

use std::time::{Duration, Instant};

use kg_engine::{
    AiConfig, AppPaths, Command, DocumentKind, Engine, ImportState, Notice, PlanPrefs, QuizStep,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("warn").init();
    dotenvy::dotenv_override().ok();
    let mut args = std::env::args().skip(1);
    let edital = args
        .next()
        .ok_or("usage: board_check <edital> <exam> [key]")?;
    let exam = args.next().ok_or("missing exam pdf")?;
    let key = args.next();
    let dir = tempfile::tempdir()?;
    let mut engine = Engine::new(
        AppPaths::new(dir.path()),
        AiConfig::from_env(|k| std::env::var(k).ok()),
    )?;

    engine.handle(Command::ImportEdital(edital.into()))?;
    wait(&mut engine, "edital", |e| {
        !matches!(e.import_state(), ImportState::Extracting { .. })
    })?;
    engine.handle(Command::GeneratePlan(PlanPrefs {
        weeks: 10,
        hours_per_week: 15,
        role: "Técnico Judiciário – Área Judiciária".into(),
    }))?;
    wait(&mut engine, "plan", |e| e.generating_since().is_none())?;
    let topics = engine.active_plan().ok_or("no plan")?.topic_count();
    println!("plan: {topics} topics");

    let t = Instant::now();
    engine.handle(Command::ImportBoard {
        path: exam.into(),
        kind: DocumentKind::Exam,
    })?;
    wait(&mut engine, "exam", |e| e.board_job().is_none())?;
    println!("exam read in {:?}", t.elapsed());
    print_notices(&mut engine);

    let plan = engine.active_plan().ok_or("no plan")?;
    let title = |id| {
        plan.find_topic(id)
            .map(|(s, t)| format!("{} — {}", s.name, t.title))
            .unwrap_or_default()
    };
    for q in engine.board_questions().iter().take(40) {
        let prompt: String = q.prompt.chars().take(90).collect();
        println!(
            "  #{:<3} {:<10} {:<55} | {}",
            q.number.unwrap_or(0),
            q.relevance.as_str(),
            q.topic_id.map(title).unwrap_or_else(|| q.reason.clone()),
            prompt.replace('\n', " ")
        );
    }

    if let Some(key) = key {
        let exam_id = engine.board_documents().first().ok_or("no document")?.id;
        engine.handle(Command::ImportAnswerKey {
            path: key.into(),
            exam: exam_id,
        })?;
        wait(&mut engine, "answer key", |e| e.board_job().is_none())?;
        print_notices(&mut engine);
    }

    let top = engine
        .active_plan()
        .ok_or("no plan")?
        .topics()
        .map(|(_, t)| (engine.board_emphasis(t.id), t.id, t.title.clone()))
        .max();
    if let Some((count, topic, name)) = top.filter(|(count, ..)| *count > 0) {
        println!("most asked topic: {name} ({count} questions)");
        engine.handle(Command::StartQuiz(topic))?;
        wait(&mut engine, "quiz", |e| {
            e.quiz().is_none_or(|q| q.step != QuizStep::Loading)
        })?;
        if let Some(quiz) = engine.quiz() {
            let real = quiz.questions().iter().filter(|q| q.from_board).count();
            println!(
                "quiz: {} questions, {real} from the exam",
                quiz.questions().len()
            );
        }
    }
    Ok(())
}

fn print_notices(engine: &mut Engine) {
    for notice in engine.take_notices() {
        println!("notice: {notice:?}");
    }
}

fn wait(
    engine: &mut Engine,
    what: &str,
    done: impl Fn(&Engine) -> bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    loop {
        engine.poll();
        if done(engine) {
            return Ok(());
        }
        let notices = engine.take_notices();
        for notice in &notices {
            if let Notice::Error(err) = notice {
                return Err(format!("{what}: {err}").into());
            }
        }
        if started.elapsed() > Duration::from_secs(900) {
            return Err(format!("{what}: timed out").into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
