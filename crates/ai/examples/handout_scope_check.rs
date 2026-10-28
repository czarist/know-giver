//! Live handout scope check; saves drafts for human review without logging credentials.
//! Usage: handout_scope_check <env-file> <new-output-directory>

use kg_ai::{AiConfig, Client, TopicContext};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let env = args.next().ok_or("missing env file")?;
    let output = std::path::PathBuf::from(args.next().ok_or("missing output directory")?);
    dotenvy::from_path_override(env)?;
    let client = Client::new(AiConfig::from_env(|key| std::env::var(key).ok()));
    std::fs::create_dir(&output)?;
    let context = TopicContext {
        exam: "Concurso de teste — Técnico Administrativo".into(),
        subject: "Língua Portuguesa".into(),
        topic: "Crase".into(),
        reference: "Questão da banca: Em 'vou à escola', explique o acento grave.".into(),
        ..TopicContext::default()
    };
    for (name, edital) in [
        (
            "multi-role",
            "CONTEÚDO PROGRAMÁTICO\nTÉCNICO ADMINISTRATIVO: Língua Portuguesa: emprego do acento indicativo de crase.\nENGENHEIRO: cálculo diferencial, integrais, resistência dos materiais. Esses assuntos não são exigidos do Técnico Administrativo.",
        ),
        (
            "truncated",
            "CONTEÚDO PROGRAMÁTICO\nTÉCNICO ADMINISTRATIVO: Língua Portuguesa: emprego do acento indicativo de crase.\n[Restante do documento não incluído neste trecho.]",
        ),
    ] {
        let draft = kg_ai::generate_handout(&client, &context, edital)?;
        let path = output.join(format!("{name}.txt"));
        std::fs::write(&path, format!("{draft:#?}"))?;
        println!("Saved {} for syllabus review", path.display());
    }
    Ok(())
}
