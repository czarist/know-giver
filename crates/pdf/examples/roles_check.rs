//! Prints the roles found in an edital's text and how much a focused prompt
//! keeps. Usage: cargo run -p kg-pdf --example roles_check -- edital.txt [role]
#![allow(clippy::unwrap_used, clippy::expect_used)]

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("text file");
    let text = kg_pdf::EditalText::from_raw(&std::fs::read_to_string(path).unwrap());
    for role in text.numbered_roles() {
        println!("{:?}: {}", role.number, role.name);
    }
    let role = args.next().map(|r| r.parse().unwrap());
    let squeeze = |s: String| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let all = squeeze(text.for_prompt(150_000));
    let focused = squeeze(text.for_prompt_role(150_000, role));
    println!(
        "prompt chars: all {} focused {}",
        all.chars().count(),
        focused.chars().count()
    );
    for marker in [
        "ESTATÍSTICA, PROGRAMAÇÃO",
        "AUDITORIA GOVERNAMENTAL:",
        "REDES, SISTEMAS",
        "ENGENHARIA DE SOFTWARE",
        "CONHECIMENTOS BÁSICOS",
        "CRONOGRAMA",
    ] {
        println!(
            "  {marker}: all={} focused={}",
            all.contains(marker),
            focused.contains(marker)
        );
    }
}
