//! Roles (cargos) offered by an edital, and syllabus text focused on one.
//!
//! Many editais cover several roles that share the basic subjects but have
//! their own specific syllabus. Their headings usually read
//! "CARGO 2: ANALISTA ... – ESPECIALIDADE: ...", sometimes wrapped onto the
//! next line.

use crate::{EditalText, fold, is_heading};

/// One role found in the edital.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    /// The number after "CARGO", when the edital numbers its roles.
    pub number: Option<u32>,
    /// Readable name, e.g. "Auditor Federal de Finanças e Controle (AFFC) –
    /// Especialidade: Tecnologia da Informação – Ciência de Dados".
    pub name: String,
}

/// Longest role name kept, in characters.
const MAX_NAME_CHARS: usize = 220;

impl EditalText {
    /// Numbered roles ("CARGO 1: …", "CARGO 2: …"). Empty when the edital
    /// does not number them this way; one entry when it has a single role.
    pub fn numbered_roles(&self) -> Vec<Role> {
        let lines: Vec<String> = self.as_str().lines().map(squeeze).collect();
        // Several mentions per role (vacancy table, syllabus): keep the
        // fullest, preferring heading lines over table cells.
        let mut best: Vec<(u32, bool, String)> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let Some((number, rest)) = role_heading(line) else {
                continue;
            };
            let heading = is_heading(line);
            let mut name = rest.to_owned();
            if heading {
                for next in lines.iter().skip(i + 1).take(2) {
                    if !is_heading(next)
                        || role_heading(next).is_some()
                        || starts_with_section_number(next)
                        || name.chars().count() > MAX_NAME_CHARS
                    {
                        break;
                    }
                    name.push(' ');
                    name.push_str(next);
                }
            }
            let name = name.trim().trim_end_matches(['.', ';', ',']).to_owned();
            if name.chars().count() < 4 {
                continue;
            }
            match best.iter_mut().find(|(n, ..)| *n == number) {
                Some(entry) => {
                    // The first heading wins: later ones (in the syllabus) may
                    // run into the subject list on the next line.
                    let better = if heading {
                        !entry.1
                    } else {
                        !entry.1 && name.chars().count() > entry.2.chars().count()
                    };
                    if better {
                        *entry = (number, heading, name);
                    }
                }
                None => best.push((number, heading, name)),
            }
        }
        best.sort_by_key(|(n, ..)| *n);
        best.into_iter()
            .map(|(number, _, name)| Role {
                number: Some(number),
                name: readable(&take(&name, MAX_NAME_CHARS)),
            })
            .collect()
    }

    /// Like [`EditalText::for_prompt`], but without the specific syllabus of
    /// the other numbered roles, so the plan only covers `role`'s content.
    pub fn for_prompt_role(&self, max_chars: usize, role: Option<u32>) -> String {
        let Some(role) = role else {
            return self.for_prompt(max_chars);
        };
        let Some(syllabus) = self.syllabus_start() else {
            return self.for_prompt(max_chars);
        };
        let text = self.as_str();
        let mut kept = String::with_capacity(text.len());
        kept.push_str(&text[..syllabus]);
        let mut skipping = false;
        for line in text[syllabus..].split_inclusive('\n') {
            let squeezed = squeeze(line);
            if is_heading(&squeezed)
                && let Some((number, _)) = role_heading(&squeezed)
            {
                skipping = number != role;
            } else if squeezed.chars().count() < 60 && fold(&squeezed).starts_with("ANEXO") {
                // Annexes (schedule, forms) follow the last role's syllabus.
                skipping = false;
            }
            if !skipping {
                kept.push_str(line);
            }
        }
        EditalText(kept).for_prompt(max_chars)
    }
}

impl EditalText {
    /// Whether most words of a role name appear in the edital, to discard
    /// names the model paraphrased or made up.
    pub fn mentions(&self, name: &str) -> bool {
        let text = squeeze(&fold(self.as_str()));
        let words: Vec<String> = fold(name)
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() >= 4)
            .map(str::to_owned)
            .collect();
        if words.is_empty() {
            return false;
        }
        let found = words.iter().filter(|w| text.contains(w.as_str())).count();
        found * 10 >= words.len() * 8
    }
}

/// `(number, rest of the line)` for "CARGO 2: ...", "2.1.2 CARGO 2 – ...".
fn role_heading(line: &str) -> Option<(u32, &str)> {
    let folded = fold(line);
    // Allow a section number such as "2.1.2" before the word.
    let skip = folded
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ' '))
        .unwrap_or(0);
    let after = folded[skip..].strip_prefix("CARGO")?;
    let after = after.trim_start();
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || digits.len() > 2 {
        return None;
    }
    let number: u32 = digits.parse().ok()?;
    let tail = after[digits.len()..].trim_start();
    let separator = tail.chars().next()?;
    if !matches!(separator, ':' | '–' | '—' | '-') {
        return None;
    }
    // `fold` keeps the length of ASCII and of most accented letters the
    // same in chars, so map back through char counts.
    let consumed = folded.chars().count() - tail.chars().count() + 1;
    let start = line.char_indices().nth(consumed).map(|(i, _)| i)?;
    Some((number, line[start..].trim()))
}

fn starts_with_section_number(line: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or("");
    !first.is_empty() && first.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Collapses runs of whitespace (tables come out with double spaces).
fn squeeze(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn take(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Words kept lowercase when a name written in capitals is made readable.
const SMALL_WORDS: &[&str] = &[
    "a", "à", "ao", "aos", "as", "às", "com", "da", "das", "de", "do", "dos", "e", "em", "na",
    "nas", "no", "nos", "o", "os", "ou", "para", "por",
];

/// "AUDITOR DE TI (AFFC)" → "Auditor de TI (AFFC)". Names that are not
/// mostly capitals are returned unchanged.
pub(crate) fn readable(name: &str) -> String {
    if !is_heading(name) {
        return name.to_owned();
    }
    name.split(' ')
        .enumerate()
        .map(|(i, word)| {
            let letters: String = word.chars().filter(|c| c.is_alphabetic()).collect();
            let lower = word.to_lowercase();
            if word.starts_with('(')
                || (letters.chars().count() <= 3 && !SMALL_WORDS.contains(&lower.as_str()))
            {
                // Acronyms: (AFFC), TI, DF.
                word.to_owned()
            } else if i > 0 && SMALL_WORDS.contains(&lower.as_str()) {
                lower
            } else {
                let mut chars = lower.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().chain(chars).collect())
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const EDITAL: &str = "\
CONCURSO PÚBLICO CGU
2.1.1 CARGO 1: AUDITOR FEDERAL DE FINANÇAS E CONTROLE (AFFC) – ESPECIALIDADE: AUDITORIA

2.1.2 CARGO 2: AUDITOR FEDERAL DE FINANÇAS E CONTROLE (AFFC) – ESPECIALIDADE: TECNOLOGIA DA
INFORMAÇÃO – CIÊNCIA DE DADOS

2.1.3 CARGO 3: AUDITOR FEDERAL DE FINANÇAS E CONTROLE (AFFC) – ESPECIALIDADE: TECNOLOGIA DA
INFORMAÇÃO – INFRAESTRUTURA DE TI
3 DOS REQUISITOS PARA A INVESTIDURA NO CARGO
Cargo  2:  Auditor  Federal  de
Finanças e Controle
15 DOS OBJETOS DE AVALIAÇÃO (HABILIDADES E CONHECIMENTOS)
CONHECIMENTOS BÁSICOS: ética pública, administração pública, políticas públicas, sociedade brasileira.
Texto longo do conteúdo básico comum a todos os cargos. Texto longo do conteúdo básico comum.
CARGO 1: AUDITOR FEDERAL DE FINANÇAS E CONTROLE (AFFC) – ESPECIALIDADE – AUDITORIA
AUDITORIA GOVERNAMENTAL: avaliação, consultoria e apuração.
CARGO  2:  AUDITOR  FEDERAL  DE  FINANÇAS  E  CONTROLE  (AFFC)  –  ESPECIALIDADE:  TECNOLOGIA  DA
INFORMAÇÃO – CIÊNCIA DE DADOS
ESTATÍSTICA, PROGRAMAÇÃO E ANÁLISE DE DADOS: Python 3, inferência.
CARGO 3: AUDITOR FEDERAL DE FINANÇAS E CONTROLE (AFFC) – ESPECIALIDADE: TECNOLOGIA DA
INFORMAÇÃO – INFRAESTRUTURA DE TI
REDES, SISTEMAS E PLATAFORMAS DE DADOS: TCP/IP.
ANEXO I
CRONOGRAMA PREVISTO
";

    fn edital() -> EditalText {
        // Long enough for the syllabus heading to count.
        EditalText::from_raw(&EDITAL.replace(
            "Texto longo do conteúdo básico comum.",
            &"Texto longo do conteúdo básico comum. ".repeat(60),
        ))
    }

    #[test]
    fn finds_numbered_roles_with_wrapped_names() {
        let roles = edital().numbered_roles();
        let names: Vec<_> = roles.iter().map(|r| (r.number, r.name.as_str())).collect();
        assert_eq!(
            names,
            [
                (
                    Some(1),
                    "Auditor Federal de Finanças e Controle (AFFC) – Especialidade: Auditoria"
                ),
                (
                    Some(2),
                    "Auditor Federal de Finanças e Controle (AFFC) – Especialidade: Tecnologia da Informação – Ciência de Dados"
                ),
                (
                    Some(3),
                    "Auditor Federal de Finanças e Controle (AFFC) – Especialidade: Tecnologia da Informação – Infraestrutura de TI"
                ),
            ]
        );
    }

    #[test]
    fn focused_prompt_drops_other_roles_specific_syllabus() {
        let text = edital().for_prompt_role(100_000, Some(2));
        assert!(text.contains("CONHECIMENTOS BÁSICOS"));
        assert!(text.contains("Python 3"));
        assert!(!text.contains("AUDITORIA GOVERNAMENTAL"));
        assert!(!text.contains("TCP/IP"));
        assert!(text.contains("CRONOGRAMA PREVISTO"));
        // Without a role nothing is dropped.
        assert!(edital().for_prompt_role(100_000, None).contains("TCP/IP"));
    }

    #[test]
    fn single_or_unnumbered_roles() {
        let text =
            EditalText::from_raw("CARGO: ANALISTA JUDICIÁRIO\nO cargo exige nível superior.");
        assert!(text.numbered_roles().is_empty());
        assert_eq!(
            readable("TÉCNICO DE TI – ÁREA ADMINISTRATIVA"),
            "Técnico de TI – Área Administrativa"
        );
        assert_eq!(readable("Analista Judiciário"), "Analista Judiciário");
        assert!(text.mentions("Analista Judiciário"));
        assert!(!text.mentions("Técnico em Enfermagem"));
    }
}
