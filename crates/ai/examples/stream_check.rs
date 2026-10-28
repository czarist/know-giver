//! Manual check of streamed tutor answers against the real endpoint in `.env`:
//! time to the first spoken sentence versus the full reply.
//!
//! cargo run -p kg-ai --example stream_check

use std::time::Instant;

use kg_ai::{AiConfig, ChatMessage, Client, TopicContext, tutor_reply_stream};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv_override().ok();
    let mut config = AiConfig::from_env(|k| std::env::var(k).ok());
    // KG_TUTOR_MODEL tries another model without editing `.env`.
    if let Ok(model) = std::env::var("KG_TUTOR_MODEL") {
        config.models.tutor = model;
    }
    let client = Client::new(config);
    let topic = TopicContext {
        exam: "Tribunal Regional – Técnico Judiciário".into(),
        subject: "Língua Portuguesa".into(),
        topic: "Emprego do sinal indicativo de crase".into(),
        ..TopicContext::default()
    };
    let history = [
        ChatMessage::assistant("Olá! Vamos estudar crase. O que você já sabe?"),
        ChatMessage::user("Quando uso crase antes de horas?"),
    ];
    let rounds: usize = std::env::var("KG_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    for _ in 1..rounds {
        // Same client: later rounds reuse the open connection.
        let started = Instant::now();
        let mut first = None;
        tutor_reply_stream(&client, &topic, &history, |_| {
            first.get_or_insert(started.elapsed());
        })?;
        println!(
            "warm-up round: first sentence after {:.2}s",
            first.map_or(0.0, |d| d.as_secs_f32())
        );
    }
    let started = Instant::now();
    let mut first = None;
    let mut count = 0;
    let reply = tutor_reply_stream(&client, &topic, &history, |sentence| {
        count += 1;
        let at = started.elapsed();
        first.get_or_insert(at);
        println!("[{:>6.2}s] {sentence}", at.as_secs_f32());
    })?;
    let total = started.elapsed();
    println!(
        "\nfirst sentence after {:.2}s, full reply after {:.2}s ({count} sentences, board {:?})",
        first.map_or(0.0, |d| d.as_secs_f32()),
        total.as_secs_f32(),
        reply.board
    );
    Ok(())
}
