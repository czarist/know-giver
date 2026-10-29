//! Manual check of quiz generation time against the real endpoint in `.env`.
//!
//! cargo run --release -p kg-ai --example quiz_check -- [count]
//!
//! Prompts are Portuguese by default; `KG_PROMPT_LANG=en` uses the English ones.

use kg_ai::{AiConfig, Client, TopicContext, generate_quiz};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "kg_ai=info".into()))
        .init();
    dotenvy::dotenv_override().ok();
    let count = std::env::args()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(5);
    let client = Client::new(AiConfig::from_env(|k| std::env::var(k).ok()))
        .with_language(kg_ai::PromptLanguage::from_env());
    let context = TopicContext {
        exam: "Tribunal Regional – Técnico Judiciário".into(),
        subject: "Língua Portuguesa".into(),
        topic: "Emprego do sinal indicativo de crase".into(),
        ..TopicContext::default()
    };
    let started = std::time::Instant::now();
    let questions = generate_quiz(&client, &context, count)?;
    println!(
        "{} of {count} questions in {:.1}s",
        questions.len(),
        started.elapsed().as_secs_f32()
    );
    for (i, q) in questions.iter().enumerate() {
        println!("{}. {}", i + 1, q.prompt);
    }
    Ok(())
}
