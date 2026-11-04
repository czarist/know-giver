//! Live check of the account services signed in on this computer, with the
//! app's own data folder (so the saved ChatGPT session is used and any
//! refreshed token is saved back): model list, a streamed lesson answer,
//! and, for services that see images, a handwritten page read with the
//! app's transcription prompt.
//!
//! cargo run -p kg-engine --example accounts_live -- [page.jpg] [chatgpt|claude-code|copilot]...

use std::time::Instant;

use kg_ai::{AccountKind, ChatMessage, Client, provider, read_manuscript_page};
use kg_engine::{AiConfig, AppPaths, Engine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let page = match args.first() {
        Some(path) if path.ends_with(".jpg") || path.ends_with(".jpeg") => {
            Some(std::fs::read(args.remove(0))?)
        }
        _ => None,
    };
    let kinds: Vec<AccountKind> = if args.is_empty() {
        AccountKind::ALL.to_vec()
    } else {
        args.iter()
            .filter_map(|id| AccountKind::from_provider_id(id))
            .collect()
    };
    // Restores the ChatGPT session saved by the app.
    let mut engine = Engine::new(AppPaths::detect(), AiConfig::default())?;
    let saved = engine.settings().clone();
    for kind in kinds {
        println!("\n=== {kind:?}: {:?}", kg_ai::account_state(kind));
        let Some(p) = provider(kind.provider_id()) else {
            continue;
        };
        let probe = Client::new(p.config(p.base_url, "", p.default_model()));
        let models = match probe.list_models() {
            Ok(models) => models,
            Err(err) => {
                println!("models: error: {err}");
                Vec::new()
            }
        };
        println!("models ({}): {models:?}", models.len());
        let model = if saved.ai_provider == p.id && !saved.ai.models.tutor.is_empty() {
            saved.ai.models.tutor.clone()
        } else if !p.default_model().is_empty() {
            p.default_model().to_owned()
        } else {
            models.first().cloned().unwrap_or_default()
        };
        if model.is_empty() {
            println!("no model to try");
            continue;
        }
        let mut config = p.config(p.base_url, "", &model);
        config.models.vision = if kind.sees_images() {
            model.clone()
        } else {
            String::new()
        };
        let client = Client::new(config);
        println!("model: {model}");
        let messages = [
            ChatMessage::system(
                "Você é um professor de Direito Administrativo. Responda em até duas frases.",
            ),
            ChatMessage::user("O que é o princípio da impessoalidade?"),
        ];
        let start = Instant::now();
        let mut first = None;
        let result = client.ask(&messages, |delta| {
            first.get_or_insert_with(|| start.elapsed());
            print!("{delta}");
        });
        match result {
            Ok(_) => println!(
                "\n[lesson ok: first word {:.1}s, total {:.1}s]",
                first.unwrap_or_default().as_secs_f32(),
                start.elapsed().as_secs_f32()
            ),
            Err(err) => println!("\n[lesson error: {err}]"),
        }
        if let Some(jpeg) = &page {
            if !kind.sees_images() {
                println!("[handwriting: skipped, this service does not see images]");
            } else {
                let start = Instant::now();
                match read_manuscript_page(&client, jpeg, 1, 1) {
                    Ok(reading) => {
                        println!(
                            "[handwriting ok in {:.1}s: {} lines, legibility {:?}, {} uncertain]",
                            start.elapsed().as_secs_f32(),
                            reading.lines.len(),
                            reading.legibility,
                            reading.uncertain.len()
                        );
                        for line in reading.lines.iter().take(4) {
                            println!("  | {line}");
                        }
                    }
                    Err(err) => println!("[handwriting error: {err}]"),
                }
            }
        }
    }
    // Saves a refreshed ChatGPT session, as the app does.
    engine.poll();
    Ok(())
}
