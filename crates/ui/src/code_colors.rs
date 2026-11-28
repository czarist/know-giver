//! Colors for code on the lesson board. One small tokenizer for the
//! languages exams ask about (Python, Java, C, JavaScript, SQL, pseudocode):
//! keywords, strings, numbers, comments, calls and operators. Each line is
//! read on its own, so a string or comment spanning lines is not followed.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

/// What a piece of a line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Keyword,
    Literal,
    Text,
    Number,
    Comment,
    Call,
    Operator,
}

const KEYWORDS: &[&str] = &[
    // Control flow and declarations shared by most languages.
    "if",
    "else",
    "elif",
    "for",
    "while",
    "do",
    "return",
    "break",
    "continue",
    "switch",
    "case",
    "default",
    "try",
    "catch",
    "except",
    "finally",
    "throw",
    "throws",
    "raise",
    "def",
    "function",
    "fn",
    "lambda",
    "class",
    "interface",
    "struct",
    "enum",
    "extends",
    "implements",
    "new",
    "import",
    "from",
    "export",
    "package",
    "public",
    "private",
    "protected",
    "static",
    "final",
    "const",
    "let",
    "var",
    "void",
    "int",
    "float",
    "double",
    "char",
    "long",
    "short",
    "boolean",
    "bool",
    "string",
    "String",
    "async",
    "await",
    "yield",
    "in",
    "is",
    "not",
    "and",
    "or",
    "with",
    "as",
    "pass",
    "global",
    "this",
    "self",
    "super",
    "match",
    "pub",
    "mut",
    "use",
    "impl",
    "typeof",
    "instanceof",
    "delete",
    "echo",
    "print",
    "printf",
    "println",
    // SQL.
    "SELECT",
    "FROM",
    "WHERE",
    "INSERT",
    "INTO",
    "VALUES",
    "UPDATE",
    "SET",
    "DELETE",
    "CREATE",
    "TABLE",
    "DROP",
    "ALTER",
    "JOIN",
    "LEFT",
    "RIGHT",
    "INNER",
    "OUTER",
    "ON",
    "GROUP",
    "BY",
    "ORDER",
    "HAVING",
    "AND",
    "OR",
    "NOT",
    "NULL",
    "AS",
    "DISTINCT",
    "LIMIT",
    "PRIMARY",
    "KEY",
    // Portuguese pseudocode (VisuAlg / Portugol).
    "se",
    "senao",
    "senão",
    "entao",
    "então",
    "fimse",
    "enquanto",
    "faca",
    "faça",
    "fimenquanto",
    "para",
    "ate",
    "até",
    "passo",
    "fimpara",
    "repita",
    "algoritmo",
    "inicio",
    "início",
    "fimalgoritmo",
    "leia",
    "escreva",
    "escreval",
    "retorne",
    "funcao",
    "função",
    "procedimento",
    "inteiro",
    "real",
    "caractere",
    "logico",
    "lógico",
    "verdadeiro",
    "falso",
    "nao",
    "não",
    "e",
    "ou",
];

const LITERALS: &[&str] = &[
    "true",
    "false",
    "null",
    "None",
    "True",
    "False",
    "undefined",
    "nil",
    "TRUE",
    "FALSE",
];

/// Splits `line` into colored pieces, covering it whole and in order.
pub fn tokens(line: &str) -> Vec<(Kind, &str)> {
    // (kind, start, end) byte ranges of `line`.
    let mut out: Vec<(Kind, usize, usize)> = Vec::new();
    let mut at = 0;
    while let Some(first) = line[at..].chars().next() {
        let rest = &line[at..];
        // "--" starts a SQL comment only before a space ("i--" is code).
        let comment = rest.starts_with("//") || rest.starts_with('#') || rest.starts_with("-- ");
        let (kind, len) = if comment {
            (Kind::Comment, rest.len())
        } else if matches!(first, '"' | '\'' | '`') {
            let close = rest[1..]
                .char_indices()
                .scan(false, |escaped, (i, c)| {
                    let found = !*escaped && c == first;
                    *escaped = !*escaped && c == '\\';
                    Some((i, found))
                })
                .find(|(_, found)| *found)
                .map_or(rest.len(), |(i, _)| i + 1 + first.len_utf8());
            (Kind::Text, close)
        } else if first.is_ascii_digit() {
            let len = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_'))
                .unwrap_or(rest.len());
            (Kind::Number, len)
        } else if first.is_alphabetic() || first == '_' {
            let len = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let word = &rest[..len];
            let kind = if KEYWORDS.contains(&word) {
                Kind::Keyword
            } else if LITERALS.contains(&word) {
                Kind::Literal
            } else if rest[len..].trim_start().starts_with('(') {
                Kind::Call
            } else {
                Kind::Plain
            };
            (kind, len)
        } else if "=+-*/%<>!&|^~?:".contains(first) {
            let len = rest
                .find(|c: char| !"=+-*/%<>!&|^~?:".contains(c))
                .unwrap_or(rest.len());
            (Kind::Operator, len)
        } else {
            (Kind::Plain, first.len_utf8())
        };
        let end = at + len.max(first.len_utf8());
        match out.last_mut() {
            Some((Kind::Plain, _, last_end)) if kind == Kind::Plain => *last_end = end,
            _ => out.push((kind, at, end)),
        }
        at = end;
    }
    out.into_iter()
        .map(|(kind, start, end)| (kind, &line[start..end]))
        .collect()
}

/// Calm colors on the board's dark code slate.
fn color(kind: Kind) -> Color32 {
    match kind {
        Kind::Plain => Color32::from_rgb(0xE8, 0xE6, 0xE0),
        Kind::Keyword => Color32::from_rgb(0xC7, 0x92, 0xEA),
        Kind::Literal | Kind::Number => Color32::from_rgb(0xF7, 0x8C, 0x6C),
        Kind::Text => Color32::from_rgb(0xC3, 0xE8, 0x8D),
        Kind::Comment => Color32::from_rgb(0x7F, 0x8C, 0x98),
        Kind::Call => Color32::from_rgb(0x82, 0xAA, 0xFF),
        Kind::Operator => Color32::from_rgb(0x89, 0xDD, 0xFF),
    }
}

/// A colored, wrapping layout of one line of code.
pub fn layout(line: &str, font: FontId, wrap: f32, opacity: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    for (kind, piece) in tokens(line) {
        job.append(
            piece,
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: color(kind).gamma_multiply(opacity),
                italics: kind == Kind::Comment,
                ..Default::default()
            },
        );
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_the_parts_of_a_line() {
        let parts = tokens("if x >= 10: print(\"ok\")  # fim");
        assert_eq!(
            parts,
            [
                (Kind::Keyword, "if"),
                (Kind::Plain, " x "),
                (Kind::Operator, ">="),
                (Kind::Plain, " "),
                (Kind::Number, "10"),
                (Kind::Operator, ":"),
                (Kind::Plain, " "),
                (Kind::Keyword, "print"),
                (Kind::Plain, "("),
                (Kind::Text, "\"ok\""),
                (Kind::Plain, ")  "),
                (Kind::Comment, "# fim"),
            ]
        );
    }

    #[test]
    fn covers_every_line_whole() {
        for line in [
            "    total = soma(a, b) // ação",
            "SELECT nome FROM aluno WHERE nota > 7;",
            "s = 'it\\'s' + \"não fecha",
            "enquanto i <= 10 faça",
            "",
        ] {
            let joined: String = tokens(line).iter().map(|(_, p)| *p).collect();
            assert_eq!(joined, line);
        }
    }

    #[test]
    fn calls_are_told_from_names() {
        assert!(tokens("total = soma(a)").contains(&(Kind::Call, "soma")));
        assert!(!tokens("total = soma").iter().any(|(k, _)| *k == Kind::Call));
    }
}
