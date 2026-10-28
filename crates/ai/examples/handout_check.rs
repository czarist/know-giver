//! Manual check of handout writing plus review against the real endpoint in
//! `.env`, on a topic where unreviewed handouts had wrong answer keys.
//!
//! cargo run -p kg-ai --example handout_check

use kg_ai::{AiConfig, Client, TopicContext, generate_handout, review_handout};

const EDITAL: &str = "EDITAL Nº 1/2026 – TRIBUNAL REGIONAL – TÉCNICO JUDICIÁRIO
ANEXO II – CONTEÚDO PROGRAMÁTICO
LÍNGUA PORTUGUESA: 1 Compreensão e interpretação de textos. 2 Emprego do sinal indicativo de crase. 3 Regência verbal e nominal.";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("kg_ai=debug")
        .init();
    dotenvy::dotenv_override().ok();
    let client = Client::new(AiConfig::from_env(|k| std::env::var(k).ok()));
    let topic = TopicContext {
        exam: "Tribunal Regional – Técnico Judiciário".into(),
        subject: "Língua Portuguesa".into(),
        topic: "Emprego do sinal indicativo de crase".into(),
        ..TopicContext::default()
    };
    let t = std::time::Instant::now();
    let draft = generate_handout(&client, &topic, EDITAL)?;
    println!(
        "written in {:?}: {} exercises",
        t.elapsed(),
        draft.exercises.len()
    );
    let t = std::time::Instant::now();
    let reviewed = review_handout(&client, &topic, EDITAL, draft)?;
    println!(
        "reviewed in {:?}: {} corrections, {} exercises dropped, {} kept",
        t.elapsed(),
        reviewed.issues.len(),
        reviewed.dropped_exercises,
        reviewed.draft.exercises.len()
    );
    for issue in &reviewed.issues {
        println!("  - {issue}");
    }
    for (i, q) in reviewed.draft.exercises.iter().enumerate() {
        println!("  {}. {} -> {}", i + 1, q.prompt, q.options[q.correct]);
    }
    Ok(())
}
