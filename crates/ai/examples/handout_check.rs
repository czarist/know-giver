//! Manual check of handout writing plus review against the real endpoint in
//! `.env`. Writes and reviews one handout per topic and reports how much the
//! review had to fix: corrections, exercises dropped by the blind solve, and
//! exercises kept. Set `MODEL_VERIFIER` to try another model for the blind solve.
//!
//! cargo run -p kg-ai --example handout_check

use kg_ai::{AiConfig, Client, TopicContext, generate_handout, review_handout};

const EDITAL: &str = "EDITAL Nº 1/2026 – TRIBUNAL REGIONAL – TÉCNICO JUDICIÁRIO
ANEXO II – CONTEÚDO PROGRAMÁTICO
LÍNGUA PORTUGUESA: 1 Compreensão e interpretação de textos. 2 Emprego do sinal indicativo de crase. 3 Regência verbal e nominal.
DIREITO CONSTITUCIONAL: 1 Princípios fundamentais. 2 Direitos e garantias fundamentais.
DIREITO ADMINISTRATIVO: 1 Atos administrativos. 2 Poderes da administração pública.";

const TOPICS: [(&str, &str); 4] = [
    ("Língua Portuguesa", "Emprego do sinal indicativo de crase"),
    ("Língua Portuguesa", "Regência verbal e nominal"),
    (
        "Direito Constitucional",
        "Direitos e garantias fundamentais",
    ),
    ("Direito Administrativo", "Atos administrativos"),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "kg_ai=info".into()))
        .init();
    dotenvy::dotenv_override().ok();
    let config = AiConfig::from_env(|k| std::env::var(k).ok());
    println!(
        "writer: {} | reviewer: {} | verifier: {}",
        config.models.for_task(kg_ai::Task::Handout),
        config.models.for_task(kg_ai::Task::Correction),
        config.models.for_task(kg_ai::Task::Verify)
    );
    let client = Client::new(config);
    let (mut corrections, mut dropped, mut kept) = (0, 0, 0);
    for (subject, topic) in TOPICS {
        let context = TopicContext {
            exam: "Tribunal Regional – Técnico Judiciário".into(),
            subject: subject.into(),
            topic: topic.into(),
            ..TopicContext::default()
        };
        let started = std::time::Instant::now();
        let draft = generate_handout(&client, &context, EDITAL)?;
        let reviewed = review_handout(&client, &context, EDITAL, draft)?;
        println!(
            "\n{topic} ({:.0}s): {} corrections, {} exercises dropped, {} kept",
            started.elapsed().as_secs_f32(),
            reviewed.issues.len(),
            reviewed.dropped_exercises,
            reviewed.draft.exercises.len()
        );
        for issue in &reviewed.issues {
            println!("  - {issue}");
        }
        corrections += reviewed.issues.len();
        dropped += reviewed.dropped_exercises;
        kept += reviewed.draft.exercises.len();
    }
    let checked = dropped + kept;
    println!(
        "\nTOTAL: {corrections} corrections in {} handouts; {dropped} of {checked} exercises dropped ({:.0}%)",
        TOPICS.len(),
        100.0 * dropped as f32 / checked.max(1) as f32
    );
    Ok(())
}
