//! Sends one lesson question through an account service signed in on this
//! computer: `cargo run -p kg-ai --example account_check -- claude-code [model]`.

use kg_ai::{AccountKind, AiConfig, ChatMessage, Client, provider};

fn main() {
    let id = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "claude-code".into());
    let Some(kind) = AccountKind::from_provider_id(&id) else {
        eprintln!("unknown account service {id}");
        std::process::exit(2);
    };
    println!("state: {:?}", kg_ai::account_state(kind));
    let Some(p) = provider(kind.provider_id()) else {
        std::process::exit(2);
    };
    let model = std::env::args()
        .nth(2)
        .unwrap_or_else(|| p.default_model().to_owned());
    let config: AiConfig = p.config(p.base_url, "", &model);
    let client = Client::new(config);
    match client.list_models() {
        Ok(models) => println!("models: {models:?}"),
        Err(err) => println!("models: {err}"),
    }
    let messages = [
        ChatMessage::system("Você é um professor de português. Responda em uma frase."),
        ChatMessage::user("O que é crase?"),
    ];
    let start = std::time::Instant::now();
    match client.ask(&messages, |delta| print!("{delta}")) {
        Ok(_) => println!("\n[ok in {:.1}s]", start.elapsed().as_secs_f32()),
        Err(err) => println!("\n[error: {err}]"),
    }
}
