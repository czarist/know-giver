//! Manual end-to-end run of the engine against the real AI endpoint in
//! `.env`: import an edital PDF, generate a plan, take a quiz, do a review.
//! Uses a temporary data folder.
//!
//! cargo run -p kg-engine --example e2e -- <edital.pdf>

use std::time::{Duration, Instant};

use kg_engine::{AiConfig, AppPaths, Command, Engine, ImportState, Notice, QuizStep, ReviewStep};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    dotenvy::dotenv_override().ok();
    let pdf = std::env::args().nth(1).ok_or("usage: e2e <edital.pdf>")?;
    let dir = tempfile::tempdir()?;
    let mut engine = Engine::new(
        AppPaths::new(dir.path()),
        AiConfig::from_env(|k| std::env::var(k).ok()),
    )?;

    engine.handle(Command::ImportEdital(pdf.into()))?;
    wait(&mut engine, "extract", |e| {
        !matches!(e.import_state(), ImportState::Extracting { .. })
    })?;
    let ImportState::Ready(edital) = engine.import_state() else {
        return Err("extraction failed".into());
    };
    println!(
        "edital: {} pages, {} chars; prompt excerpt {} chars",
        edital.pages,
        edital.text.char_count(),
        edital.text.for_prompt(150_000).chars().count()
    );

    let started = Instant::now();
    engine.handle(Command::GeneratePlan(kg_engine::PlanPrefs {
        weeks: 10,
        hours_per_week: 15,
        role: "Técnico Judiciário – Área Judiciária".into(),
    }))?;
    wait(&mut engine, "plan", |e| e.generating_since().is_none())?;
    let plan = engine.active_plan().ok_or("no plan created")?;
    println!(
        "plan in {:?}: {} | {} | {} subjects, {} topics",
        started.elapsed(),
        plan.title,
        plan.exam,
        plan.subjects.len(),
        plan.topic_count()
    );
    for s in &plan.subjects {
        println!("  {} ({})", s.name, s.topics.len());
    }
    let topic = plan.subjects[0].topics[0].id;

    engine.handle(Command::StartQuiz(topic))?;
    wait(&mut engine, "quiz", |e| {
        e.quiz().is_some_and(|q| q.step != QuizStep::Loading) || e.quiz().is_none()
    })?;
    let questions = engine.quiz().map_or(0, |q| q.questions().len());
    println!("quiz: {questions} questions");
    for _ in 0..questions {
        engine.handle(Command::AnswerQuiz(0))?;
        engine.handle(Command::NextQuestion)?;
    }
    println!("quiz step: {:?}", engine.quiz().map(|q| q.step));

    engine.handle(Command::StartReview(topic))?;
    wait(&mut engine, "review question", |e| {
        e.review().is_none_or(|r| !r.is_loading())
    })?;
    if let Some(ReviewStep::Answering { question }) = engine.review().map(|r| r.step.clone()) {
        println!("review question: {question}");
        engine.handle(Command::SubmitReviewAnswer(
            "Não lembro bem, acho que trata das regras básicas do tema.".into(),
        ))?;
        wait(&mut engine, "review grade", |e| {
            e.review().is_none_or(|r| !r.is_loading())
        })?;
        if let Some(ReviewStep::Graded {
            feedback,
            suggested,
            ..
        }) = engine.review().map(|r| r.step.clone())
        {
            println!(
                "review: {} / {:?} / {}",
                feedback.score, suggested, feedback.feedback
            );
            engine.handle(Command::GradeReview(suggested))?;
        }
    }
    for notice in engine.take_notices() {
        println!("notice: {notice:?}");
    }
    let plan = engine.active_plan().ok_or("plan vanished")?;
    let t = plan.find_topic(topic).ok_or("topic vanished")?.1;
    println!("topic after: {:?} due {:?}", t.status, t.review.due);
    Ok(())
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
        for notice in engine.take_notices() {
            if let Notice::Error(err) = notice {
                return Err(format!("{what}: {err}").into());
            }
        }
        if started.elapsed() > Duration::from_secs(600) {
            return Err(format!("{what}: timed out").into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
