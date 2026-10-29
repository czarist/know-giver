//! Written answers: essays, technical discursive questions and case studies.
//!
//! A [`Proposal`] is fixed once created: its statement and [`Rubric`] are the
//! version every attempt on it is graded against. An [`Attempt`] holds the
//! photographed pages, the transcript the model read (kept apart from the
//! one the student confirmed) and, last, the [`Evaluation`].
//!
//! Everything here is pure rules with no I/O; [`crate::store`] persists it.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::plan::{Difficulty, PlanId, TopicId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProposalId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AttemptId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PageId(pub i64);

/// Most pages one attempt can have.
pub const MAX_PAGES: usize = 8;

/// Kind of written answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WritingKind {
    /// Redação dissertativo-argumentativa.
    #[default]
    Essay,
    /// Questão discursiva técnica.
    Technical,
    /// Estudo de caso.
    CaseStudy,
}

impl WritingKind {
    pub const ALL: [Self; 3] = [Self::Essay, Self::Technical, Self::CaseStudy];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Essay => "essay",
            Self::Technical => "technical",
            Self::CaseStudy => "case",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == value)
    }

    /// Portuguese name, for prompts.
    pub fn prompt_name(self) -> &'static str {
        match self {
            Self::Essay => "redação dissertativo-argumentativa",
            Self::Technical => "questão discursiva técnica",
            Self::CaseStudy => "estudo de caso",
        }
    }
}

/// Where a rubric's criteria came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RubricSource {
    /// Written in the exam notice or board material (with the passage).
    Edital,
    /// Typed by the student.
    Manual,
    /// Know Giver's own criteria for the kind of answer. Not an official
    /// rubric of any exam board.
    Generic,
}

impl RubricSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Edital => "edital",
            Self::Manual => "manual",
            Self::Generic => "generic",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Edital, Self::Manual, Self::Generic]
            .into_iter()
            .find(|s| s.as_str() == value)
    }
}

/// One thing the answer is graded on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Criterion {
    /// Stable id inside the rubric ("c1", or "generic.essay.theme").
    pub key: String,
    /// Name in Portuguese, as the evaluator reads it.
    pub name: String,
    pub description: String,
    /// Points this criterion is worth; also its weight in the total.
    pub max_points: f64,
}

/// The criteria an attempt is graded on, frozen with the proposal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rubric {
    pub criteria: Vec<Criterion>,
    pub source: RubricSource,
    /// For [`RubricSource::Edital`], the passage of the notice the criteria
    /// come from; empty otherwise.
    pub evidence: String,
}

/// Why a rubric cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RubricError {
    #[error("rubric has no criteria")]
    Empty,
    #[error("rubric has more than {MAX_CRITERIA} criteria")]
    TooMany,
    #[error("criterion {0} has no name")]
    NoName(usize),
    #[error("criterion {0} has invalid points")]
    BadPoints(usize),
    #[error("criterion '{0}' appears twice")]
    Duplicate(String),
    #[error("line {0} of the manual rubric is not 'name: points'")]
    BadLine(usize),
}

pub const MAX_CRITERIA: usize = 10;
/// Most points one rubric can be worth.
pub const MAX_RUBRIC_POINTS: f64 = 1000.0;

impl Rubric {
    pub fn max_total(&self) -> f64 {
        self.criteria.iter().map(|c| c.max_points).sum()
    }

    /// Checks names, points and limits.
    pub fn validate(&self) -> Result<(), RubricError> {
        if self.criteria.is_empty() {
            return Err(RubricError::Empty);
        }
        if self.criteria.len() > MAX_CRITERIA {
            return Err(RubricError::TooMany);
        }
        let mut seen = std::collections::HashSet::new();
        for (i, c) in self.criteria.iter().enumerate() {
            if c.name.trim().is_empty() {
                return Err(RubricError::NoName(i + 1));
            }
            if !c.max_points.is_finite() || c.max_points <= 0.0 || c.max_points > MAX_RUBRIC_POINTS
            {
                return Err(RubricError::BadPoints(i + 1));
            }
            if !seen.insert(c.key.clone()) || c.key.trim().is_empty() {
                return Err(RubricError::Duplicate(c.name.clone()));
            }
        }
        if self.max_total() > MAX_RUBRIC_POINTS {
            return Err(RubricError::BadPoints(self.criteria.len()));
        }
        Ok(())
    }

    /// Fingerprint of what is graded and how many points each thing is
    /// worth. Scores are comparable only between rubrics with the same key.
    pub fn key(&self) -> String {
        let mut text = String::from(self.source.as_str());
        for c in &self.criteria {
            text.push('|');
            text.push_str(&normalize(&c.name));
            text.push(':');
            text.push_str(&format!("{:.2}", c.max_points));
        }
        format!("{:016x}", fnv1a(&text))
    }
}

/// FNV-1a, for fingerprints that stay the same across runs and builds.
pub fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Know Giver's criteria for each kind of answer, worth 100 points. They
/// follow what discursive tests usually grade and are not any exam board's
/// official rubric (the ENEM grid is not used).
pub fn generic_rubric(kind: WritingKind) -> Rubric {
    let criterion = |key: &str, name: &str, description: &str, max_points: f64| Criterion {
        key: format!("generic.{}.{key}", kind.as_str()),
        name: name.into(),
        description: description.into(),
        max_points,
    };
    let language = criterion(
        "language",
        "Gramática, ortografia e pontuação",
        "Norma-padrão: concordância, regência, ortografia, acentuação e pontuação.",
        15.0,
    );
    let criteria = match kind {
        WritingKind::Essay => vec![
            criterion(
                "task",
                "Atendimento ao tema e ao tipo de texto",
                "Trata do tema proposto, sem fugir dele, em texto dissertativo-argumentativo.",
                20.0,
            ),
            criterion(
                "argument",
                "Argumentação",
                "Tese clara, argumentos desenvolvidos e fundamentados, com repertório pertinente.",
                30.0,
            ),
            criterion(
                "organization",
                "Organização, coerência e coesão",
                "Introdução, desenvolvimento e conclusão; progressão das ideias; conectivos adequados.",
                20.0,
            ),
            criterion(
                "content",
                "Domínio do conteúdo",
                "Informações corretas e relacionadas aos tópicos estudados.",
                15.0,
            ),
            language,
        ],
        WritingKind::Technical => vec![
            criterion(
                "task",
                "Atendimento ao enunciado",
                "Responde a todos os itens pedidos, na ordem e no formato pedidos.",
                20.0,
            ),
            criterion(
                "content",
                "Domínio do conteúdo técnico",
                "Conceitos, regras e fundamentos corretos e precisos.",
                35.0,
            ),
            criterion(
                "reasoning",
                "Raciocínio técnico e fundamentação",
                "Justifica as afirmações e aplica o conteúdo ao que foi perguntado.",
                20.0,
            ),
            criterion(
                "organization",
                "Organização, coerência e coesão",
                "Texto claro, em parágrafos, com ideias encadeadas.",
                10.0,
            ),
            language,
        ],
        WritingKind::CaseStudy => vec![
            criterion(
                "task",
                "Atendimento ao caso",
                "Identifica o problema do caso e responde ao que foi pedido.",
                20.0,
            ),
            criterion(
                "content",
                "Domínio do conteúdo",
                "Usa conceitos e normas corretos e pertinentes ao caso.",
                25.0,
            ),
            criterion(
                "analysis",
                "Análise e solução",
                "Aplica o conteúdo aos fatos do caso e propõe solução fundamentada.",
                30.0,
            ),
            criterion(
                "organization",
                "Organização, coerência e coesão",
                "Texto claro, em parágrafos, com ideias encadeadas.",
                10.0,
            ),
            language,
        ],
    };
    Rubric {
        criteria,
        source: RubricSource::Generic,
        evidence: String::new(),
    }
}

/// Reads criteria typed by the student, one per line: `name: points` or
/// `name: points: description` (`|` or ` - ` also separate).
pub fn parse_manual_rubric(text: &str) -> Result<Rubric, RubricError> {
    let mut criteria = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim().trim_start_matches(['-', '•', '*']).trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line
            .split(['|', ':', ';'])
            .flat_map(|p| p.split(" - "))
            .map(str::trim)
            .collect();
        let points = parts
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(at, p)| parse_points(p).map(|v| (at, v)));
        let Some((at, max_points)) = points else {
            return Err(RubricError::BadLine(i + 1));
        };
        let name = parts[..at].join(" ").trim().to_owned();
        let description = parts[at + 1..].join(" ").trim().to_owned();
        criteria.push(Criterion {
            key: format!("c{}", criteria.len() + 1),
            name,
            description,
            max_points,
        });
    }
    let rubric = Rubric {
        criteria,
        source: RubricSource::Manual,
        evidence: String::new(),
    };
    rubric.validate()?;
    Ok(rubric)
}

fn parse_points(text: &str) -> Option<f64> {
    let text = text
        .trim()
        .trim_end_matches(|c: char| c.is_alphabetic() || c == '.')
        .trim()
        .replace(',', ".");
    text.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// A writing task, fixed once created.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub id: ProposalId,
    pub plan_id: PlanId,
    pub kind: WritingKind,
    pub difficulty: Difficulty,
    pub theme: String,
    pub statement: String,
    pub instructions: String,
    /// Plan topics the proposal is about.
    pub topics: Vec<TopicId>,
    pub line_limit: Option<u32>,
    pub minutes: Option<u32>,
    pub rubric: Rubric,
    /// What a full answer covers. Shown only after an attempt is graded.
    pub expected_points: Vec<String>,
    pub model: String,
    pub prompt_version: String,
    pub created_at: NaiveDateTime,
}

/// A proposal before it is saved.
#[derive(Debug, Clone, PartialEq)]
pub struct NewProposal {
    pub kind: WritingKind,
    pub difficulty: Difficulty,
    pub theme: String,
    pub statement: String,
    pub instructions: String,
    pub topics: Vec<TopicId>,
    pub line_limit: Option<u32>,
    pub minutes: Option<u32>,
    pub rubric: Rubric,
    pub expected_points: Vec<String>,
    pub model: String,
    pub prompt_version: String,
}

/// Where an attempt is in its processing. Saved, so a closed app picks up
/// where it stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttemptStatus {
    /// Pages being added and arranged.
    Draft,
    /// The model is reading the pages.
    Transcribing,
    /// A page must be photographed again; no grade is given.
    Retake,
    /// The transcript waits for the student to check it.
    Review,
    /// The model is grading the confirmed transcript.
    Evaluating,
    Evaluated,
    /// Not graded: too much of the text could not be read for a fair grade.
    Pending,
    /// Processing failed; it can be run again from [`Attempt::failed_stage`].
    Failed,
}

impl AttemptStatus {
    const ALL: [Self; 8] = [
        Self::Draft,
        Self::Transcribing,
        Self::Retake,
        Self::Review,
        Self::Evaluating,
        Self::Evaluated,
        Self::Pending,
        Self::Failed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Transcribing => "transcribing",
            Self::Retake => "retake",
            Self::Review => "review",
            Self::Evaluating => "evaluating",
            Self::Evaluated => "evaluated",
            Self::Pending => "pending",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == value)
    }

    /// Work runs in the background in this state.
    pub fn is_processing(self) -> bool {
        matches!(self, Self::Transcribing | Self::Evaluating)
    }

    /// Pages can still be added, moved or replaced.
    pub fn pages_editable(self) -> bool {
        matches!(self, Self::Draft | Self::Retake)
    }
}

/// Processing step that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    Transcription,
    Evaluation,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Transcription => "transcription",
            Self::Evaluation => "evaluation",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Transcription, Self::Evaluation]
            .into_iter()
            .find(|s| s.as_str() == value)
    }
}

/// How an attempt's text was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AttemptOrigin {
    /// Handwritten and photographed.
    #[default]
    Photos,
    /// A rewritten version of another attempt's text, typed while checking
    /// its transcript. The original attempt stays as it was.
    Rewrite,
}

impl AttemptOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Photos => "photos",
            Self::Rewrite => "rewrite",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Photos, Self::Rewrite]
            .into_iter()
            .find(|s| s.as_str() == value)
    }
}

/// One try at a proposal.
#[derive(Debug, Clone, PartialEq)]
pub struct Attempt {
    pub id: AttemptId,
    pub proposal_id: ProposalId,
    /// 1 for the first attempt on the proposal.
    pub number: u32,
    /// The attempt this one rewrites.
    pub parent: Option<AttemptId>,
    pub origin: AttemptOrigin,
    pub status: AttemptStatus,
    pub failed_stage: Option<Stage>,
    /// Why it failed, is pending or needs new photos (for the student).
    pub note: String,
    /// Bumped each time background work starts, so a late result of an
    /// earlier run is not applied.
    pub run: u32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    /// Estimated total and maximum, once evaluated.
    pub score: Option<(f64, f64)>,
}

/// Something about a photo that may make it hard to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageIssue {
    Dark,
    Overexposed,
    Blurry,
    LowContrast,
    LowResolution,
    /// The model said text runs off the edge of the photo.
    Cropped,
    /// The model could not read the page well enough.
    Illegible,
}

impl PageIssue {
    /// A page with this issue is not sent: it must be photographed again.
    pub fn blocks(self) -> bool {
        matches!(self, Self::LowResolution)
    }
}

/// A photographed page.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub id: PageId,
    pub attempt_id: AttemptId,
    /// 0-based order in the attempt.
    pub position: u32,
    /// The file as the student gave it, copied into the data folder.
    pub original_path: String,
    /// Rotated, resized JPEG sent to the model.
    pub processed_path: String,
    /// Quarter turns clockwise applied on top of the photo's own orientation.
    pub rotation: u8,
    pub width: u32,
    pub height: u32,
    pub issues: Vec<PageIssue>,
}

/// A page about to be saved.
#[derive(Debug, Clone, PartialEq)]
pub struct NewPage {
    pub original_path: String,
    pub processed_path: String,
    pub rotation: u8,
    pub width: u32,
    pub height: u32,
    pub issues: Vec<PageIssue>,
}

/// Marks used in transcripts. The text between them is the student's own.
pub mod marks {
    /// `[?palavra?]`: read, but not sure.
    pub const UNCERTAIN_OPEN: &str = "[?";
    pub const UNCERTAIN_CLOSE: &str = "?]";
    /// A word or passage that could not be read.
    pub const ILLEGIBLE: &str = "[ilegível]";
    /// `[rasura: texto]`: crossed out by the student (not part of the answer).
    pub const STRUCK_OPEN: &str = "[rasura";
}

/// How easy a page was to read, as the model saw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Legibility {
    #[default]
    Good,
    Fair,
    Poor,
}

/// A passage the model was not sure about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Uncertain {
    /// 1-based line on the page, when known.
    pub line: Option<u32>,
    pub text: String,
    pub note: String,
}

/// One page as the model read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptPage {
    /// 1-based page number.
    pub page: u32,
    /// Lines as written, with [`marks`]; spelling and errors kept.
    pub lines: Vec<String>,
    pub uncertain: Vec<Uncertain>,
    pub legibility: Legibility,
    /// What the model noticed about the photo or the writing.
    pub warnings: Vec<String>,
    /// Why the page must be photographed again, when it must.
    pub retake: Option<String>,
}

/// The model's reading of every page, kept as it came.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Transcript {
    pub pages: Vec<TranscriptPage>,
}

impl Transcript {
    /// Each page's text, lines joined.
    pub fn page_texts(&self) -> Vec<String> {
        self.pages.iter().map(|p| p.lines.join("\n")).collect()
    }

    /// Pages that must be photographed again, with the reason.
    pub fn retakes(&self) -> Vec<(u32, String)> {
        self.pages
            .iter()
            .filter_map(|p| p.retake.clone().map(|r| (p.page, r)))
            .collect()
    }
}

/// A line the student corrected while checking the transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    pub page: u32,
    /// 1-based line, or `None` when the page's lines were added or removed
    /// and the change is recorded for the whole page.
    pub line: Option<u32>,
    pub before: String,
    pub after: String,
}

/// Lines that differ between what the model read and what the student
/// confirmed, page by page.
pub fn edits(original: &Transcript, confirmed: &[String]) -> Vec<Edit> {
    let mut out = Vec::new();
    for (i, page) in original.pages.iter().enumerate() {
        let after = confirmed.get(i).map(String::as_str).unwrap_or_default();
        let after_lines: Vec<&str> = after.lines().collect();
        if after_lines.len() == page.lines.len() {
            for (n, (before, after)) in page.lines.iter().zip(&after_lines).enumerate() {
                if before.trim_end() != after.trim_end() {
                    out.push(Edit {
                        page: page.page,
                        line: Some(n as u32 + 1),
                        before: before.clone(),
                        after: (*after).to_owned(),
                    });
                }
            }
        } else {
            out.push(Edit {
                page: page.page,
                line: None,
                before: page.lines.join("\n"),
                after: after.to_owned(),
            });
        }
    }
    out
}

/// Share of words changed between the model's reading and the confirmed
/// text (word-level edit distance), from 0 to 1.
pub fn change_ratio(original: &Transcript, confirmed: &[String]) -> f64 {
    let before: Vec<String> = original
        .page_texts()
        .iter()
        .flat_map(|t| words(t))
        .collect();
    let after: Vec<String> = confirmed.iter().flat_map(|t| words(t)).collect();
    let longest = before.len().max(after.len());
    if longest == 0 {
        return 0.0;
    }
    levenshtein(&before, &after) as f64 / longest as f64
}

/// Above this share of changed words, checking the transcript became
/// rewriting the answer, which is saved as a new version instead.
pub const REWRITE_RATIO: f64 = 0.25;

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_owned).collect()
}

fn levenshtein(a: &[String], b: &[String]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut current = vec![i + 1; b.len() + 1];
        for (j, y) in b.iter().enumerate() {
            let cost = usize::from(x != y);
            current[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        previous = current;
    }
    previous[b.len()]
}

/// How much of a text could not be read for sure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Unreadable {
    /// Words of the answer (crossed-out passages left out).
    pub words: usize,
    /// Words inside `[?…?]`.
    pub uncertain: usize,
    /// `[ilegível]` marks.
    pub illegible: usize,
}

/// Fewest words for a fair grade.
pub const MIN_WORDS: usize = 15;
/// Largest share of uncertain or illegible words for a fair grade.
pub const MAX_UNREADABLE: f64 = 0.10;

impl Unreadable {
    pub fn of(pages: &[String]) -> Self {
        let mut out = Self::default();
        for page in pages {
            let text = strip_struck(page);
            let mut rest = text.as_str();
            while let Some(start) = rest.find(marks::UNCERTAIN_OPEN) {
                let after = &rest[start + marks::UNCERTAIN_OPEN.len()..];
                let Some(end) = after.find(marks::UNCERTAIN_CLOSE) else {
                    break;
                };
                out.uncertain += after[..end].split_whitespace().count().max(1);
                rest = &after[end + marks::UNCERTAIN_CLOSE.len()..];
            }
            out.illegible += text.matches(marks::ILLEGIBLE).count();
            out.words += text
                .replace(marks::ILLEGIBLE, " x ")
                .replace(marks::UNCERTAIN_OPEN, " ")
                .replace(marks::UNCERTAIN_CLOSE, " ")
                .split_whitespace()
                .filter(|w| w.chars().any(char::is_alphanumeric))
                .count();
        }
        out
    }

    /// Share of the words not read for sure.
    pub fn share(self) -> f64 {
        if self.words == 0 {
            return 1.0;
        }
        (self.uncertain + self.illegible) as f64 / self.words as f64
    }

    /// Enough was read to grade the answer without guessing.
    pub fn fair_to_grade(self) -> bool {
        self.words >= MIN_WORDS && self.share() <= MAX_UNREADABLE
    }
}

/// The text without `[rasura…]` passages.
pub fn strip_struck(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(marks::STRUCK_OPEN) {
        out.push_str(&rest[..start]);
        match rest[start..].find(']') {
            Some(end) => rest = &rest[start + end + 1..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// The transcript the student confirmed.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ConfirmedTranscript {
    /// Each page's text.
    pub pages: Vec<String>,
    pub edits: Vec<Edit>,
}

/// Saved transcript of an attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptRecord {
    /// What the model read; never changed.
    pub original: Transcript,
    pub confirmed: Option<ConfirmedTranscript>,
    pub model: String,
    pub prompt_version: String,
    pub created_at: NaiveDateTime,
    pub confirmed_at: Option<NaiveDateTime>,
}

/// A passage of the student's text an observation is based on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// Exactly as in the confirmed transcript.
    pub quote: String,
    pub page: Option<u32>,
    pub line: Option<u32>,
}

/// Score and reasons for one criterion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CriterionScore {
    pub key: String,
    pub score: f64,
    pub justification: String,
    pub evidence: Vec<Evidence>,
}

/// What kind of problem an observation is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProblemCategory {
    /// Grammar, spelling, punctuation.
    Writing,
    /// What is said: wrong, missing or off-topic content.
    Content,
    /// Organization, coherence, cohesion, argument.
    Structure,
}

impl ProblemCategory {
    pub const ALL: [Self; 3] = [Self::Writing, Self::Content, Self::Structure];
}

/// Something that went well.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remark {
    pub text: String,
    pub evidence: Option<Evidence>,
}

/// Something to fix, with how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    pub category: ProblemCategory,
    pub text: String,
    pub suggestion: String,
    pub evidence: Option<Evidence>,
}

/// A formative, AI-estimated evaluation of an attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evaluation {
    /// The rubric as it was when graded.
    pub rubric: Rubric,
    pub scores: Vec<CriterionScore>,
    pub strengths: Vec<Remark>,
    pub problems: Vec<Problem>,
    /// Three things to work on first.
    pub priorities: Vec<String>,
    /// Plan topics worth reviewing.
    pub review_topics: Vec<TopicId>,
    /// Quotes the model gave that are not in the text, left out.
    pub dropped_quotes: u32,
    pub model: String,
    pub prompt_version: String,
    pub evaluated_at: NaiveDateTime,
}

impl Evaluation {
    /// Sum of the criteria scores; always computed, never taken from the model.
    pub fn total(&self) -> f64 {
        self.scores.iter().map(|s| s.score).sum()
    }

    pub fn max_total(&self) -> f64 {
        self.rubric.max_total()
    }

    /// Score of a criterion as a share of its points, 0 to 1.
    pub fn share(&self, key: &str) -> Option<f64> {
        let max = self
            .rubric
            .criteria
            .iter()
            .find(|c| c.key == key)?
            .max_points;
        let score = self.scores.iter().find(|s| s.key == key)?.score;
        Some(score / max)
    }
}

impl Serialize for TopicId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i64(self.0)
    }
}

impl<'de> Deserialize<'de> for TopicId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        i64::deserialize(d).map(TopicId)
    }
}

/// A difficulty seen in more than one attempt, with where it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecurringIssue {
    pub label: String,
    pub category: ProblemCategory,
    pub advice: String,
    pub examples: Vec<IssueExample>,
}

/// A passage of an attempt showing a recurring difficulty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueExample {
    pub attempt: AttemptId,
    pub quote: String,
}

/// The latest look at recurring difficulties in a plan's attempts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DifficultyReport {
    pub issues: Vec<RecurringIssue>,
    pub attempts: Vec<AttemptId>,
    pub model: String,
    pub prompt_version: String,
    pub created_at: NaiveDateTime,
}

/// Lowercase text without accents, marks or extra spaces, for comparing
/// quotes with the transcript.
pub fn normalize(text: &str) -> String {
    let text = text
        .replace(marks::UNCERTAIN_OPEN, " ")
        .replace(marks::UNCERTAIN_CLOSE, " ");
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'ê' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            '“' | '”' | '«' | '»' => '"',
            '‘' | '’' => '\'',
            '–' | '—' => '-',
            c => c,
        };
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with(' ') && !out.is_empty() {
            out.push(' ');
        }
    }
    out.trim_end().to_owned()
}

/// Where a quote is in the pages (1-based page and line), or `None` when
/// the text does not contain it. Line breaks inside the quote are allowed.
pub fn locate(pages: &[String], quote: &str) -> Option<(u32, u32)> {
    let needle = normalize(quote);
    if needle.chars().count() < 3 {
        return None;
    }
    for (p, page) in pages.iter().enumerate() {
        let mut joined = String::new();
        let mut starts = Vec::new();
        for line in page.lines() {
            starts.push(joined.len());
            let line = normalize(line);
            joined.push_str(&line);
            joined.push(' ');
        }
        if let Some(at) = joined.find(&needle) {
            let line = starts.iter().rposition(|&s| s <= at).unwrap_or(0);
            return Some((p as u32 + 1, line as u32 + 1));
        }
    }
    None
}

/// What the exam notice says about written tests: which kinds it has and
/// the passages describing them (sent to the model to read criteria from).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WritingRules {
    /// Kinds the notice mentions; empty when it has no written test.
    pub kinds: Vec<WritingKind>,
    /// Passages of the notice around those mentions.
    pub excerpt: String,
}

impl WritingRules {
    pub fn has_written_test(&self) -> bool {
        !self.kinds.is_empty()
    }
}

/// Phrases (normalized) that announce each kind of written test. Plain
/// "redação" is left out: notices say "com redação dada pela Lei…" and list
/// "Redação Oficial" as a syllabus topic.
const WRITING_MARKERS: [(WritingKind, &[&str]); 3] = [
    (
        WritingKind::Essay,
        &[
            "prova de redacao",
            "provas de redacao",
            "redacao dissertativ",
            "texto dissertativo",
            "avaliacao da redacao",
            "correcao da redacao",
            "nota da redacao",
            "nota na redacao",
            "redacao de texto",
        ],
    ),
    (
        WritingKind::Technical,
        &[
            "prova discursiva",
            "provas discursivas",
            "questao discursiva",
            "questoes discursivas",
            "prova dissertativa",
            "provas dissertativas",
            "questoes dissertativas",
            "questao dissertativa",
        ],
    ),
    (
        WritingKind::CaseStudy,
        &["estudo de caso", "estudos de caso", "situacao problema"],
    ),
];

/// Lines of the notice kept around each mention.
const RULES_WINDOW: usize = 10;
/// Longest excerpt sent to the model, in characters.
const RULES_MAX: usize = 9_000;

/// Finds the written tests an exam notice describes.
pub fn writing_rules(edital: &str) -> WritingRules {
    let lines: Vec<&str> = edital.lines().collect();
    let mut kinds = Vec::new();
    let mut hits = Vec::new();
    for i in 0..lines.len() {
        // A phrase may break across two lines.
        let pair = match lines.get(i + 1) {
            Some(next) => normalize(&format!("{} {next}", lines[i])),
            None => normalize(lines[i]),
        };
        for (kind, phrases) in WRITING_MARKERS {
            if phrases.iter().any(|p| pair.contains(p)) {
                if !kinds.contains(&kind) {
                    kinds.push(kind);
                }
                hits.push(i);
            }
        }
    }
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for i in hits {
        let (start, end) = (
            i.saturating_sub(RULES_WINDOW),
            (i + RULES_WINDOW + 1).min(lines.len()),
        );
        match spans.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => spans.push((start, end)),
        }
    }
    let mut excerpt = String::new();
    for (start, end) in spans {
        let piece = lines[start..end].join(
            "
",
        );
        if excerpt.chars().count() + piece.chars().count() > RULES_MAX {
            break;
        }
        if !excerpt.is_empty() {
            excerpt.push_str(
                "
[…]
",
            );
        }
        excerpt.push_str(piece.trim());
    }
    kinds.sort_by_key(|k| WritingKind::ALL.iter().position(|x| x == k));
    WritingRules { kinds, excerpt }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_tests_are_found_only_when_the_notice_has_them() {
        let none = "Art. 5º com a redação dada pela Lei 8.112. CONTEÚDO: Redação Oficial; crase.";
        assert!(!writing_rules(none).has_written_test());
        let essay = "11. DA PROVA DISCURSIVA\n11.1 A prova discursiva consistirá na redação de texto \
                     dissertativo, de até 30 linhas. 11.2 Serão avaliados a apresentação, a \
                     estrutura textual e o desenvolvimento do tema.";
        let rules = writing_rules(essay);
        assert_eq!(
            rules.kinds,
            vec![WritingKind::Essay, WritingKind::Technical]
        );
        assert!(rules.excerpt.contains("30 linhas"));
        assert!(rules.excerpt.contains("estrutura textual"));
        let case = "A prova discursiva será composta de 2 estudos de caso.";
        assert_eq!(
            writing_rules(case).kinds,
            vec![WritingKind::Technical, WritingKind::CaseStudy]
        );
    }

    fn page(n: u32, lines: &[&str]) -> TranscriptPage {
        TranscriptPage {
            page: n,
            lines: lines.iter().map(|l| (*l).to_owned()).collect(),
            uncertain: Vec::new(),
            legibility: Legibility::Good,
            warnings: Vec::new(),
            retake: None,
        }
    }

    #[test]
    fn generic_rubrics_are_valid_and_worth_100() {
        for kind in WritingKind::ALL {
            let rubric = generic_rubric(kind);
            assert_eq!(rubric.validate(), Ok(()));
            assert!((rubric.max_total() - 100.0).abs() < 1e-9);
            assert_eq!(rubric.source, RubricSource::Generic);
        }
        // Different kinds are graded differently and do not compare.
        assert_ne!(
            generic_rubric(WritingKind::Essay).key(),
            generic_rubric(WritingKind::Technical).key()
        );
        assert_eq!(
            generic_rubric(WritingKind::Essay).key(),
            generic_rubric(WritingKind::Essay).key()
        );
    }

    #[test]
    fn manual_rubric_reads_lines_and_rejects_bad_ones() {
        let rubric = parse_manual_rubric(
            "Conteúdo: 6,5\n- Estrutura | 2 | parágrafos\n\nLíngua - 1.5 pts\n",
        )
        .unwrap();
        assert_eq!(rubric.criteria.len(), 3);
        assert_eq!(rubric.criteria[0].name, "Conteúdo");
        assert!((rubric.criteria[0].max_points - 6.5).abs() < 1e-9);
        assert_eq!(rubric.criteria[1].description, "parágrafos");
        assert!((rubric.max_total() - 10.0).abs() < 1e-9);
        assert_eq!(rubric.source, RubricSource::Manual);
        assert_eq!(
            parse_manual_rubric("Conteúdo\nLíngua: 2"),
            Err(RubricError::BadLine(1))
        );
        assert_eq!(parse_manual_rubric(""), Err(RubricError::Empty));
        assert_eq!(
            parse_manual_rubric("Conteúdo: 0"),
            Err(RubricError::BadPoints(1))
        );
    }

    #[test]
    fn total_is_the_sum_of_criteria() {
        let rubric = generic_rubric(WritingKind::Technical);
        let scores = rubric
            .criteria
            .iter()
            .map(|c| CriterionScore {
                key: c.key.clone(),
                score: c.max_points / 2.0,
                justification: String::new(),
                evidence: Vec::new(),
            })
            .collect();
        let eval = Evaluation {
            rubric,
            scores,
            strengths: Vec::new(),
            problems: Vec::new(),
            priorities: Vec::new(),
            review_topics: Vec::new(),
            dropped_quotes: 0,
            model: "m".into(),
            prompt_version: "p".into(),
            evaluated_at: NaiveDateTime::default(),
        };
        assert!((eval.total() - 50.0).abs() < 1e-9);
        assert!((eval.max_total() - 100.0).abs() < 1e-9);
        assert_eq!(eval.share("generic.technical.content"), Some(0.5));
    }

    #[test]
    fn uncertain_text_is_counted_and_struck_text_left_out() {
        let pages = vec![
            "A democracia [?pressupõe?] a participação [ilegível] dos cidadãos\n[rasura: errado] e o voto."
                .to_owned(),
        ];
        let u = Unreadable::of(&pages);
        assert_eq!(u.uncertain, 1);
        assert_eq!(u.illegible, 1);
        // "[ilegível]" counts as a word; the crossed-out one does not.
        assert_eq!(u.words, 11);
        assert!(!u.fair_to_grade(), "too short and too unsure");
        let long = vec!["palavra ".repeat(40) + "[?talvez?]"];
        assert!(Unreadable::of(&long).fair_to_grade());
        let unsure = vec!["[?palavra?] ".repeat(10) + &"certa ".repeat(20)];
        assert!(!Unreadable::of(&unsure).fair_to_grade());
    }

    #[test]
    fn edits_are_kept_per_line_and_rewrites_detected() {
        let original = Transcript {
            pages: vec![page(1, &["A crase ocore", "antes de palavra feminina."])],
        };
        let fixed = vec!["A crase ocorre\nantes de palavra feminina.".to_owned()];
        let edits = edits(&original, &fixed);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].line, Some(1));
        assert_eq!(edits[0].before, "A crase ocore");
        assert!(change_ratio(&original, &fixed) < REWRITE_RATIO);
        let rewritten = vec!["Texto totalmente novo sobre outro assunto qualquer.".to_owned()];
        assert!(change_ratio(&original, &rewritten) > REWRITE_RATIO);
        assert_eq!(super::edits(&original, &rewritten)[0].line, None);
    }

    #[test]
    fn quotes_are_found_with_page_and_line_or_not_at_all() {
        let pages = vec![
            "Introdução ao tema.\nA crase é a fusão\nde duas vogais.".to_owned(),
            "Conclusão: o [?uso?] é obrigatório.".to_owned(),
        ];
        assert_eq!(locate(&pages, "a fusão de duas vogais"), Some((1, 2)));
        assert_eq!(locate(&pages, "“o uso é OBRIGATÓRIO”"), Some((2, 1)));
        assert_eq!(locate(&pages, "texto que o aluno não escreveu"), None);
        assert_eq!(locate(&pages, "a"), None);
    }

    #[test]
    fn statuses_round_trip() {
        for status in AttemptStatus::ALL {
            assert_eq!(AttemptStatus::parse(status.as_str()), Some(status));
        }
        assert!(AttemptStatus::Transcribing.is_processing());
        assert!(AttemptStatus::Retake.pages_editable());
        assert!(!AttemptStatus::Review.pages_editable());
    }
}
