//! Manual check against the real endpoint configured in `.env`.
//!
//! cargo run -p kg-ai --example live_check

use kg_ai::{
    AiConfig, ChatMessage, Client, PlanPrefs, TopicContext, evaluate_review, generate_plan,
    generate_quiz, review_question, start_lesson, tutor_reply,
};

const EDITAL: &str = "EDITAL Nº 1/2026 – TRIBUNAL REGIONAL FEDERAL – TÉCNICO JUDICIÁRIO
ANEXO II – CONTEÚDO PROGRAMÁTICO
LÍNGUA PORTUGUESA: 1 Compreensão e interpretação de textos. 2 Ortografia oficial. 3 Acentuação gráfica. 4 Emprego do sinal indicativo de crase. 5 Regência verbal e nominal. 6 Concordância verbal e nominal. 7 Pontuação.
DIREITO CONSTITUCIONAL: 1 Princípios fundamentais. 2 Direitos e garantias fundamentais. 3 Organização do Estado. 4 Poder Judiciário.
NOÇÕES DE INFORMÁTICA: 1 Sistemas operacionais. 2 Editores de texto e planilhas. 3 Segurança da informação.";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("kg_ai=debug")
        .init();
    // The project .env wins over global variables of the same name.
    dotenvy::dotenv_override().ok();
    let client = Client::new(AiConfig::from_env(|k| std::env::var(k).ok()));

    let t = std::time::Instant::now();
    client.ping()?;
    println!("ping ok ({:?})", t.elapsed());

    let t = std::time::Instant::now();
    let plan = generate_plan(
        &client,
        EDITAL,
        &PlanPrefs {
            weeks: 6,
            hours_per_week: 10,
            role: String::new(),
        },
    )?;
    println!("plan ({:?}): {} / {}", t.elapsed(), plan.title, plan.exam);
    for s in &plan.subjects {
        println!("  {} ({} tópicos)", s.name, s.topics.len());
        for topic in &s.topics {
            println!("    s{} [{}] {}", topic.week, topic.difficulty, topic.title);
        }
    }

    let ctx = TopicContext {
        exam: plan.exam.clone(),
        subject: "Língua Portuguesa".into(),
        topic: "Emprego do sinal indicativo de crase".into(),
    };

    let t = std::time::Instant::now();
    let opening = start_lesson(&client, &ctx)?;
    println!("\nlesson start ({:?}): {opening}", t.elapsed());
    let history = [
        ChatMessage::assistant(opening),
        ChatMessage::user(
            "Sei que crase é a junção de a com a, mas não sei quando usar antes de horas.",
        ),
    ];
    let t = std::time::Instant::now();
    println!(
        "\nreply ({:?}): {}",
        t.elapsed(),
        tutor_reply(&client, &ctx, &history)?
    );

    let t = std::time::Instant::now();
    let quiz = generate_quiz(&client, &ctx, 3)?;
    println!("\nquiz ({:?}):", t.elapsed());
    for q in &quiz {
        println!("  {} -> {:?} correct={}", q.prompt, q.options, q.correct);
    }

    let question = review_question(&client, &ctx)?;
    println!("\nreview question: {question}");
    let feedback = evaluate_review(
        &client,
        &ctx,
        &question,
        "Usa crase antes de palavra feminina quando o verbo pede a preposição a.",
    )?;
    println!("feedback: {} / {}", feedback.score, feedback.feedback);
    Ok(())
}
