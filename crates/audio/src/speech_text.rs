//! Rewrites tutor text so a Brazilian Portuguese TTS reads it naturally.
//!
//! The TTS reads what it sees: "art. 5º, § 2º da CF/88" comes out as letters
//! and symbols. This expands legal abbreviations, ordinals, dates and law
//! numbers into the words a teacher would say. The Portuguese words below are
//! speech data, not UI text.

use std::sync::LazyLock;

use regex::{Captures, Regex};

/// `(pattern, replacement)` applied in order. Patterns use `\b` where the
/// abbreviation could also be the start of a longer word.
const ABBREVIATIONS: &[(&str, &str)] = &[
    (r"§§\s*", "parágrafos "),
    (r"§\s*", "parágrafo "),
    (r"\b[Aa]rts\.\s*", "artigos "),
    (r"\b[Aa]rt\.\s*", "artigo "),
    (r"\b[Ii]ncs?\.\s*", "inciso "),
    (r"\b[Aa]l\.\s*", "alínea "),
    (r"\b[Nn][º°]\s*", "número "),
    (r"\b[Pp]\.\s?ex\.", "por exemplo"),
    (r"\betc\.", "etcétera"),
    (r"\b[Ss]r\.", "senhor"),
    (r"\b[Ss]ra\.", "senhora"),
    (r"\b[Dd]r\.", "doutor"),
    (r"\b[Dd]ra\.", "doutora"),
    (r"\be/ou\b", "e ou"),
    (r"\bCF\s*/\s*(?:19)?88\b", "Constituição Federal de 1988"),
    (r"\bCF\b", "Constituição Federal"),
    (r"\bCPC\b", "Código de Processo Civil"),
    (r"\bCPP\b", "Código de Processo Penal"),
    (r"\bCTN\b", "Código Tributário Nacional"),
    (r"\bCC\b", "Código Civil"),
    (r"\bCP\b", "Código Penal"),
    (
        r"\bLINDB\b",
        "Lei de Introdução às Normas do Direito Brasileiro",
    ),
    (r"\bEC\b", "Emenda Constitucional"),
    (r"\bLC\b", "Lei Complementar"),
    (r"\bSTF\b", "Supremo Tribunal Federal"),
    (r"\bSTJ\b", "Superior Tribunal de Justiça"),
    (r"\bTST\b", "Tribunal Superior do Trabalho"),
];

const MONTHS: [&str; 12] = [
    "janeiro",
    "fevereiro",
    "março",
    "abril",
    "maio",
    "junho",
    "julho",
    "agosto",
    "setembro",
    "outubro",
    "novembro",
    "dezembro",
];

const UNITS_ORD: [&str; 10] = [
    "", "primeiro", "segundo", "terceiro", "quarto", "quinto", "sexto", "sétimo", "oitavo", "nono",
];
const TENS_ORD: [&str; 10] = [
    "",
    "décimo",
    "vigésimo",
    "trigésimo",
    "quadragésimo",
    "quinquagésimo",
    "sexagésimo",
    "septuagésimo",
    "octogésimo",
    "nonagésimo",
];

struct Rules {
    abbreviations: Vec<(Regex, &'static str)>,
    ordinal: Option<Regex>,
    date: Option<Regex>,
    thousands: Option<Regex>,
    law_year: Option<Regex>,
    roman: Option<Regex>,
    percent: Option<Regex>,
    money: Option<Regex>,
    brackets: Option<Regex>,
    dashes: Option<Regex>,
    comma_runs: Option<Regex>,
    spaces: Option<Regex>,
    space_before_punct: Option<Regex>,
}

/// Compiled once. All patterns are constants: a pattern that fails to
/// compile is skipped here and caught by `all_patterns_compile`.
static RULES: LazyLock<Rules> = LazyLock::new(|| {
    let re = |pattern: &str| Regex::new(pattern).ok();
    Rules {
        abbreviations: ABBREVIATIONS
            .iter()
            .filter_map(|(p, r)| Regex::new(p).ok().map(|re| (re, *r)))
            .collect(),
        ordinal: re(r"\b(\d{1,2})\s?([º°ª])"),
        date: re(r"\b(\d{1,2})/(\d{1,2})/(\d{4}|\d{2})\b"),
        thousands: re(r"\b\d{1,3}(?:\.\d{3})+\b"),
        law_year: re(r"\b(\d+)/(\d{4}|\d{2})\b"),
        roman: re(r"\b(inciso|incisos|título|capítulo|seção|livro|parte|anexo)\s+([IVXLC]{1,7})\b"),
        percent: re(r"(\d)\s?%"),
        money: re(r"R\$\s?(\d[\d.,]*)"),
        brackets: re(r"[()\[\]]"),
        dashes: re(r"\s[–—-]\s|[–—]"),
        comma_runs: re(r"(?:\s*,\s*){2,}"),
        spaces: re(r"[ 	]+"),
        // Also drops a comma left before a period: "(servidores)." → "servidores."
        space_before_punct: re(r"\s*,?\s+([,.;:!?])|,([.;:!?])"),
    }
});

/// Applies `rule` to `text` when the rule compiled.
fn apply<R: regex::Replacer>(rule: Option<&Regex>, text: String, replacement: R) -> String {
    match rule {
        Some(re) => re.replace_all(&text, replacement).into_owned(),
        None => text,
    }
}

/// Rewrites `text` for speech. Safe to call on any text.
pub fn normalize_for_speech(text: &str) -> String {
    let r = &*RULES;
    let mut out = apply(r.date.as_ref(), text.to_owned(), |c: &Captures<'_>| {
        let day: u32 = c[1].parse().unwrap_or(0);
        let month: usize = c[2].parse().unwrap_or(0);
        let year = expand_year(&c[3]);
        match MONTHS.get(month.wrapping_sub(1)) {
            Some(name) if (1..=31).contains(&day) => {
                let day = if day == 1 {
                    "primeiro".to_owned()
                } else {
                    day.to_string()
                };
                format!("{day} de {name} de {year}")
            }
            _ => c[0].to_owned(),
        }
    });

    for (pattern, replacement) in &r.abbreviations {
        out = apply(Some(pattern), out, *replacement);
    }

    out = apply(r.ordinal.as_ref(), out, |c: &Captures<'_>| {
        let n: usize = c[1].parse().unwrap_or(0);
        ordinal(n, &c[2] == "ª").unwrap_or_else(|| c[0].to_owned())
    });
    out = apply(
        r.roman.as_ref(),
        out,
        |c: &Captures<'_>| match roman_to_int(&c[2]) {
            Some(n) => format!("{} {n}", &c[1]),
            None => c[0].to_owned(),
        },
    );
    out = apply(r.thousands.as_ref(), out, |c: &Captures<'_>| {
        c[0].replace('.', "")
    });
    out = apply(r.law_year.as_ref(), out, |c: &Captures<'_>| {
        format!("{} de {}", &c[1], expand_year(&c[2]))
    });
    out = apply(r.percent.as_ref(), out, "$1 por cento");
    out = apply(r.money.as_ref(), out, "$1 reais");

    out = apply(r.brackets.as_ref(), out, ", ");
    out = apply(r.dashes.as_ref(), out, ", ");
    out = apply(r.comma_runs.as_ref(), out, ", ");
    out = apply(r.spaces.as_ref(), out, " ");
    out = apply(r.space_before_punct.as_ref(), out, "$1$2");
    out.trim().trim_start_matches(',').trim().to_owned()
}

/// Two-digit years from laws are read in full: "90" → "1990", "05" → "2005".
fn expand_year(year: &str) -> String {
    match year.len() {
        2 => {
            let y: u32 = year.parse().unwrap_or(0);
            if y <= 40 {
                format!("20{year}")
            } else {
                format!("19{year}")
            }
        }
        _ => year.to_owned(),
    }
}

/// Ordinal in words, 1..=99. Feminine for "ª".
fn ordinal(n: usize, feminine: bool) -> Option<String> {
    if n == 0 || n > 99 {
        return None;
    }
    let words: Vec<&str> = [TENS_ORD[n / 10], UNITS_ORD[n % 10]]
        .into_iter()
        .filter(|w| !w.is_empty())
        .collect();
    let words: Vec<String> = words
        .into_iter()
        .map(|w| {
            if feminine {
                format!("{}a", w.trim_end_matches('o'))
            } else {
                w.to_owned()
            }
        })
        .collect();
    Some(words.join(" "))
}

fn roman_to_int(roman: &str) -> Option<u32> {
    let value = |c| match c {
        'I' => Some(1),
        'V' => Some(5),
        'X' => Some(10),
        'L' => Some(50),
        'C' => Some(100),
        _ => None,
    };
    let digits: Vec<u32> = roman.chars().map(value).collect::<Option<_>>()?;
    let mut total = 0i64;
    for (i, &d) in digits.iter().enumerate() {
        let next = digits.get(i + 1).copied().unwrap_or(0);
        if d < next {
            total -= i64::from(d);
        } else {
            total += i64::from(d);
        }
    }
    u32::try_from(total).ok().filter(|n| *n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(text: &str) -> String {
        normalize_for_speech(text)
    }

    #[test]
    fn all_patterns_compile() {
        let r = &*RULES;
        assert_eq!(r.abbreviations.len(), ABBREVIATIONS.len());
        for rule in [
            &r.ordinal,
            &r.date,
            &r.thousands,
            &r.law_year,
            &r.roman,
            &r.percent,
            &r.money,
            &r.brackets,
            &r.dashes,
            &r.comma_runs,
            &r.spaces,
            &r.space_before_punct,
        ] {
            assert!(rule.is_some());
        }
    }

    #[test]
    fn legal_citation() {
        assert_eq!(
            n("Veja o art. 5º, § 2º, da CF/88."),
            "Veja o artigo quinto, parágrafo segundo, da Constituição Federal de 1988."
        );
    }

    #[test]
    fn law_numbers_and_years() {
        assert_eq!(
            n("A Lei nº 8.112/90 trata dos servidores."),
            "A Lei número 8112 de 1990 trata dos servidores."
        );
        assert_eq!(n("a Lei 14.133/2021"), "a Lei 14133 de 2021");
    }

    #[test]
    fn incisos_in_roman_numerals() {
        assert_eq!(n("o inciso IV do art. 37"), "o inciso 4 do artigo 37");
        assert_eq!(n("Título II"), "Título II", "only lowercase keywords");
        assert_eq!(n("o título III"), "o título 3");
    }

    #[test]
    fn ordinals_masculine_and_feminine() {
        assert_eq!(n("1º lugar"), "primeiro lugar");
        assert_eq!(n("a 2ª fase"), "a segunda fase");
        assert_eq!(n("o 21º artigo"), "o vigésimo primeiro artigo");
        assert_eq!(n("a 10ª Vara"), "a décima Vara");
    }

    #[test]
    fn dates() {
        assert_eq!(n("prova em 15/03/2026"), "prova em 15 de março de 2026");
        assert_eq!(n("em 01/01/2025"), "em primeiro de janeiro de 2025");
    }

    #[test]
    fn symbols_and_brackets() {
        assert_eq!(n("a Lei 8.112 (servidores)."), "a Lei 8112, servidores.");
        assert_eq!(n("veja (art. 37)!"), "veja, artigo 37!");
        assert_eq!(
            n("cerca de 30% das questões"),
            "cerca de 30 por cento das questões"
        );
        assert_eq!(
            n("o prazo (em regra) é de 5 dias"),
            "o prazo, em regra, é de 5 dias"
        );
        assert_eq!(n("custa R$ 50"), "custa 50 reais");
        assert_eq!(n("isso — e só isso — vale"), "isso, e só isso, vale");
    }

    #[test]
    fn plain_text_is_unchanged() {
        let text = "A crase é a fusão da preposição a com o artigo a.";
        assert_eq!(n(text), text);
    }

    #[test]
    fn roman_conversion() {
        assert_eq!(roman_to_int("XIV"), Some(14));
        assert_eq!(roman_to_int("XL"), Some(40));
        assert_eq!(roman_to_int("ABC"), None);
    }
}
