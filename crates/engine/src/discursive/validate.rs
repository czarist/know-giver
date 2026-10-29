//! Turns the model's drafts into stored data, checking them against what
//! is known: the notice, the rubric, the confirmed text and the plan.

use chrono::NaiveDateTime;
use kg_ai::{EvaluationDraft, EvidenceDraft, IssueDraft, PageReading, ProposalDraft};
use kg_study::discursive::{generic_rubric, locate, normalize, parse_manual_rubric};
use kg_study::{
    AttemptId, Criterion, CriterionScore, Difficulty, Evaluation, Evidence, IssueExample,
    Legibility, NewProposal, PageIssue, Problem, ProblemCategory, RecurringIssue, Remark, Rubric,
    RubricSource, TopicId, TranscriptPage, Uncertain, WritingKind,
};

/// Fewest characters of the notice a rubric's evidence must quote.
const MIN_EVIDENCE_CHARS: usize = 25;

/// What the student asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalRequest {
    pub kind: WritingKind,
    pub difficulty: Difficulty,
    /// Topics to write about; empty to let the app suggest.
    pub topics: Vec<TopicId>,
    /// Criteria typed by the student (`name: points` per line); `None` to
    /// use the notice's or the generic ones.
    pub manual_rubric: Option<String>,
    pub line_limit: Option<u32>,
    pub minutes: Option<u32>,
}

/// Why a draft cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Invalid(pub String);

/// Builds the proposal to save from the model's draft. `candidates` are
/// the topics numbered in the prompt; `edital` the notice passages sent.
pub(crate) fn proposal(
    draft: ProposalDraft,
    request: &ProposalRequest,
    candidates: &[TopicId],
    edital: &str,
    model: &str,
) -> Result<NewProposal, Invalid> {
    let statement = draft.statement.trim().to_owned();
    if statement.chars().count() < 20 {
        return Err(Invalid("statement missing or too short".into()));
    }
    let expected: Vec<String> = draft
        .expected_points
        .iter()
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty())
        .take(10)
        .collect();
    if expected.is_empty() {
        return Err(Invalid("no expected points".into()));
    }
    let rubric = match &request.manual_rubric {
        Some(text) => parse_manual_rubric(text).map_err(|e| Invalid(e.to_string()))?,
        None => edital_rubric(&draft, edital).unwrap_or_else(|| generic_rubric(request.kind)),
    };
    let mut topics: Vec<TopicId> = draft
        .topics
        .iter()
        .filter_map(|n| n.checked_sub(1).and_then(|i| candidates.get(i)).copied())
        .collect();
    topics.dedup();
    if topics.is_empty() {
        topics = if request.topics.is_empty() {
            candidates.iter().take(1).copied().collect()
        } else {
            request.topics.clone()
        };
    }
    // Limits come from the student or, when written in the notice, from it.
    let in_notice =
        |value: Option<u32>| value.filter(|v| !edital.is_empty() && mentions_number(edital, *v));
    Ok(NewProposal {
        kind: request.kind,
        difficulty: request.difficulty,
        theme: draft.theme.trim().to_owned(),
        statement,
        instructions: draft.instructions.trim().to_owned(),
        topics,
        line_limit: request.line_limit.or_else(|| in_notice(draft.line_limit)),
        minutes: request.minutes.or_else(|| in_notice(draft.minutes)),
        rubric,
        expected_points: expected,
        model: model.to_owned(),
        prompt_version: kg_ai::prompt_version("proposal"),
    })
}

/// The notice's criteria, when the model quoted the passage that defines
/// them and that passage is really in the notice.
fn edital_rubric(draft: &ProposalDraft, edital: &str) -> Option<Rubric> {
    let rubric = &draft.rubric;
    let evidence = normalize(&rubric.evidence);
    if !rubric.from_edital
        || evidence.chars().count() < MIN_EVIDENCE_CHARS
        || !normalize(edital).contains(&evidence)
    {
        return None;
    }
    let built = Rubric {
        criteria: rubric
            .criteria
            .iter()
            .enumerate()
            .map(|(i, c)| Criterion {
                key: format!("c{}", i + 1),
                name: c.name.trim().to_owned(),
                description: c.description.trim().to_owned(),
                max_points: c.points,
            })
            .collect(),
        source: RubricSource::Edital,
        evidence: rubric.evidence.trim().to_owned(),
    };
    built.validate().ok().map(|()| built)
}

fn mentions_number(text: &str, value: u32) -> bool {
    let value = value.to_string();
    text.split(|c: char| !c.is_ascii_digit())
        .any(|n| n == value)
}

/// What an evaluation run produced.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Outcome {
    Graded(Box<Evaluation>),
    /// Not graded, with the model's reason.
    Pending(String),
}

/// Checks a graded draft: every criterion once, scores within limits,
/// quotes that exist (located again in the text) and plan topics.
pub(crate) fn evaluation(
    draft: EvaluationDraft,
    rubric: &Rubric,
    pages: &[String],
    topics: &[TopicId],
    model: &str,
    now: NaiveDateTime,
) -> Result<Outcome, Invalid> {
    if draft.pending {
        return Ok(Outcome::Pending(draft.pending_reason.trim().to_owned()));
    }
    let mut dropped = 0_u32;
    let mut evidence = |e: &EvidenceDraft| -> Option<Evidence> {
        let quote = e.quote.trim();
        if quote.is_empty() {
            return None;
        }
        match locate(pages, quote) {
            Some((page, line)) => Some(Evidence {
                quote: quote.to_owned(),
                page: Some(page),
                line: Some(line),
            }),
            None => {
                dropped += 1;
                None
            }
        }
    };
    let mut scores = Vec::with_capacity(rubric.criteria.len());
    for criterion in &rubric.criteria {
        let mut found = draft
            .criteria
            .iter()
            .filter(|s| s.key.trim().eq_ignore_ascii_case(&criterion.key));
        let Some(score) = found.next() else {
            return Err(Invalid(format!("criterion {} missing", criterion.key)));
        };
        if found.next().is_some() {
            return Err(Invalid(format!("criterion {} graded twice", criterion.key)));
        }
        let value = score
            .score
            .filter(|v| v.is_finite() && *v >= 0.0 && *v <= criterion.max_points + 1e-9)
            .ok_or_else(|| Invalid(format!("score of {} out of range", criterion.key)))?;
        scores.push(CriterionScore {
            key: criterion.key.clone(),
            score: value,
            justification: score.justification.trim().to_owned(),
            evidence: score.evidence.iter().filter_map(&mut evidence).collect(),
        });
    }
    let strengths = draft
        .strengths
        .iter()
        .filter(|r| !r.text.trim().is_empty())
        .map(|r| Remark {
            text: r.text.trim().to_owned(),
            evidence: evidence(&r.evidence),
        })
        .collect();
    let problems = draft
        .problems
        .iter()
        .filter(|p| !p.text.trim().is_empty())
        .map(|p| Problem {
            category: category(&p.category),
            text: p.text.trim().to_owned(),
            suggestion: p.suggestion.trim().to_owned(),
            evidence: evidence(&p.evidence),
        })
        .collect();
    let mut review_topics: Vec<TopicId> = draft
        .review_topics
        .iter()
        .filter_map(|n| n.checked_sub(1).and_then(|i| topics.get(i)).copied())
        .collect();
    review_topics.sort_unstable();
    review_topics.dedup();
    Ok(Outcome::Graded(Box::new(Evaluation {
        rubric: rubric.clone(),
        scores,
        strengths,
        problems,
        priorities: draft
            .priorities
            .iter()
            .map(|p| p.trim().to_owned())
            .filter(|p| !p.is_empty())
            .take(3)
            .collect(),
        review_topics,
        dropped_quotes: dropped,
        model: model.to_owned(),
        prompt_version: kg_ai::prompt_version("evaluate"),
        evaluated_at: now,
    })))
}

pub(crate) fn category(value: &str) -> ProblemCategory {
    match normalize(value).as_str() {
        "escrita" | "writing" | "gramatica" | "ortografia" => ProblemCategory::Writing,
        "estrutura" | "structure" | "organizacao" | "coesao" | "coerencia" => {
            ProblemCategory::Structure
        }
        _ => ProblemCategory::Content,
    }
}

/// A page reading, with its issues for the page list.
pub(crate) fn page(reading: PageReading, number: u32) -> (TranscriptPage, Vec<PageIssue>) {
    let legibility = match normalize(&reading.legibility).as_str() {
        "ruim" | "poor" => Legibility::Poor,
        "regular" | "fair" | "media" => Legibility::Fair,
        _ => Legibility::Good,
    };
    let mut issues = Vec::new();
    if reading.cropped {
        issues.push(PageIssue::Cropped);
    }
    if legibility == Legibility::Poor {
        issues.push(PageIssue::Illegible);
    }
    let retake = Some(reading.retake.trim().to_owned()).filter(|r| !r.is_empty());
    let lines = reading.lines.len() as u32;
    (
        TranscriptPage {
            page: number,
            lines: reading.lines,
            uncertain: reading
                .uncertain
                .into_iter()
                .filter(|u| !u.text.trim().is_empty())
                .map(|u| Uncertain {
                    line: u.line.filter(|l| *l >= 1 && *l <= lines),
                    text: u.text,
                    note: u.note,
                })
                .collect(),
            legibility,
            warnings: reading.warnings,
            retake,
        },
        issues,
    )
}

/// Recurring difficulties whose examples are in the attempts they name;
/// those left with examples from fewer than two attempts are dropped.
pub(crate) fn issues(
    drafts: Vec<IssueDraft>,
    attempts: &[(AttemptId, Vec<String>)],
) -> Vec<RecurringIssue> {
    drafts
        .into_iter()
        .filter_map(|d| {
            let examples: Vec<IssueExample> = d
                .examples
                .iter()
                .filter_map(|e| {
                    let (id, pages) = attempts.get(e.attempt.checked_sub(1)?)?;
                    locate(pages, &e.quote).map(|_| IssueExample {
                        attempt: *id,
                        quote: e.quote.trim().to_owned(),
                    })
                })
                .collect();
            let mut distinct: Vec<AttemptId> = examples.iter().map(|e| e.attempt).collect();
            distinct.sort_unstable();
            distinct.dedup();
            (distinct.len() >= 2 && !d.label.trim().is_empty()).then(|| RecurringIssue {
                label: d.label.trim().to_owned(),
                category: category(&d.category),
                advice: d.advice.trim().to_owned(),
                examples,
            })
        })
        .take(6)
        .collect()
}

#[cfg(test)]
mod tests {
    use kg_ai::{CriterionDraft, IssueExampleDraft, ProblemDraft, RubricDraft, ScoreDraft};

    use super::*;

    fn request() -> ProposalRequest {
        ProposalRequest {
            kind: WritingKind::Essay,
            difficulty: Difficulty::Medium,
            topics: Vec::new(),
            manual_rubric: None,
            line_limit: None,
            minutes: None,
        }
    }

    fn proposal_draft(rubric: RubricDraft) -> ProposalDraft {
        ProposalDraft {
            theme: "Participação social".into(),
            statement: "Redija um texto dissertativo sobre a participação social na democracia."
                .into(),
            instructions: "Dê um título.".into(),
            line_limit: Some(30),
            minutes: Some(90),
            topics: vec![2, 9],
            rubric,
            expected_points: vec!["conceito de democracia".into(), " ".into()],
        }
    }

    const EDITAL: &str = "11.3 A redação valerá 20 pontos e terá de 20 a 30 linhas. \
        11.4 Serão avaliados: apresentação e estrutura textual (5 pontos) e \
        desenvolvimento do tema (15 pontos).";

    #[test]
    fn notice_criteria_need_a_real_passage() {
        let rubric = RubricDraft {
            from_edital: true,
            evidence: "Serão avaliados: apresentação e estrutura textual (5 pontos)".into(),
            criteria: vec![
                CriterionDraft {
                    name: "Apresentação e estrutura textual".into(),
                    description: String::new(),
                    points: 5.0,
                },
                CriterionDraft {
                    name: "Desenvolvimento do tema".into(),
                    description: String::new(),
                    points: 15.0,
                },
            ],
        };
        let topics = [TopicId(1), TopicId(2)];
        let made = proposal(
            proposal_draft(rubric.clone()),
            &request(),
            &topics,
            EDITAL,
            "m",
        )
        .unwrap();
        assert_eq!(made.rubric.source, RubricSource::Edital);
        assert!((made.rubric.max_total() - 20.0).abs() < 1e-9);
        assert_eq!(made.topics, vec![TopicId(2)], "unknown topic 9 dropped");
        assert_eq!(made.line_limit, Some(30), "30 is in the notice");
        assert_eq!(made.minutes, None, "90 minutes is not in the notice");
        assert_eq!(made.expected_points.len(), 1);

        // A passage the notice does not have: the generic rubric is used.
        let invented = RubricDraft {
            evidence: "Serão avaliadas as cinco competências do ENEM com 200 pontos".into(),
            ..rubric
        };
        let made = proposal(proposal_draft(invented), &request(), &topics, EDITAL, "m").unwrap();
        assert_eq!(made.rubric.source, RubricSource::Generic);
        // Without a notice, limits from the model are not used.
        let made = proposal(
            proposal_draft(RubricDraft::default()),
            &request(),
            &topics,
            "",
            "m",
        )
        .unwrap();
        assert_eq!((made.line_limit, made.minutes), (None, None));
    }

    #[test]
    fn manual_criteria_win_and_bad_drafts_are_refused() {
        let mut req = request();
        req.manual_rubric = Some("Conteúdo: 8\nLíngua: 2".into());
        req.line_limit = Some(15);
        let made = proposal(
            proposal_draft(RubricDraft::default()),
            &req,
            &[TopicId(1)],
            EDITAL,
            "m",
        )
        .unwrap();
        assert_eq!(made.rubric.source, RubricSource::Manual);
        assert_eq!(made.line_limit, Some(15));
        let mut empty = proposal_draft(RubricDraft::default());
        empty.statement = "Curto".into();
        assert!(proposal(empty, &request(), &[TopicId(1)], "", "m").is_err());
    }

    fn rubric() -> Rubric {
        kg_study::discursive::parse_manual_rubric("Conteúdo: 6\nLíngua: 4").unwrap()
    }

    fn score(key: &str, value: f64, quote: &str) -> ScoreDraft {
        ScoreDraft {
            key: key.into(),
            score: Some(value),
            justification: "ok".into(),
            evidence: vec![EvidenceDraft {
                quote: quote.into(),
                page: Some(9),
                line: Some(99),
            }],
        }
    }

    fn pages() -> Vec<String> {
        vec!["A democracia exige\nparticipação dos cidadãos.".into()]
    }

    #[test]
    fn total_comes_from_criteria_and_quotes_are_checked() {
        let draft = EvaluationDraft {
            criteria: vec![
                score("c1", 4.5, "exige participação"),
                score("C2", 3.0, "frase que não existe no texto"),
            ],
            problems: vec![ProblemDraft {
                category: "escrita".into(),
                text: "Falta vírgula".into(),
                suggestion: "Use vírgula".into(),
                evidence: EvidenceDraft {
                    quote: "democracia exige".into(),
                    page: None,
                    line: None,
                },
            }],
            priorities: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            review_topics: vec![1, 5],
            ..EvaluationDraft::default()
        };
        let Outcome::Graded(eval) = evaluation(
            draft,
            &rubric(),
            &pages(),
            &[TopicId(7)],
            "m",
            NaiveDateTime::default(),
        )
        .unwrap() else {
            panic!("expected a grade");
        };
        assert!((eval.total() - 7.5).abs() < 1e-9);
        assert!((eval.max_total() - 10.0).abs() < 1e-9);
        // The model's page 9 line 99 was replaced by where the quote is.
        assert_eq!(eval.scores[0].evidence[0].page, Some(1));
        assert_eq!(eval.scores[0].evidence[0].line, Some(1));
        assert!(eval.scores[1].evidence.is_empty(), "invented quote dropped");
        assert_eq!(eval.dropped_quotes, 1);
        assert_eq!(eval.problems[0].category, ProblemCategory::Writing);
        assert_eq!(eval.priorities.len(), 3);
        assert_eq!(eval.review_topics, vec![TopicId(7)]);
    }

    #[test]
    fn invalid_grades_are_refused() {
        let over = EvaluationDraft {
            criteria: vec![score("c1", 7.0, ""), score("c2", 1.0, "")],
            ..EvaluationDraft::default()
        };
        assert!(
            evaluation(
                over,
                &rubric(),
                &pages(),
                &[],
                "m",
                NaiveDateTime::default()
            )
            .is_err()
        );
        let missing = EvaluationDraft {
            criteria: vec![score("c1", 1.0, "")],
            ..EvaluationDraft::default()
        };
        assert!(
            evaluation(
                missing,
                &rubric(),
                &pages(),
                &[],
                "m",
                NaiveDateTime::default()
            )
            .is_err()
        );
        let twice = EvaluationDraft {
            criteria: vec![
                score("c1", 1.0, ""),
                score("c1", 1.0, ""),
                score("c2", 1.0, ""),
            ],
            ..EvaluationDraft::default()
        };
        assert!(
            evaluation(
                twice,
                &rubric(),
                &pages(),
                &[],
                "m",
                NaiveDateTime::default()
            )
            .is_err()
        );
        let no_score = EvaluationDraft {
            criteria: vec![
                ScoreDraft {
                    score: None,
                    ..score("c1", 0.0, "")
                },
                score("c2", 1.0, ""),
            ],
            ..EvaluationDraft::default()
        };
        assert!(
            evaluation(
                no_score,
                &rubric(),
                &pages(),
                &[],
                "m",
                NaiveDateTime::default()
            )
            .is_err()
        );
        let pending = EvaluationDraft {
            pending: true,
            pending_reason: "metade ilegível".into(),
            ..EvaluationDraft::default()
        };
        assert_eq!(
            evaluation(
                pending,
                &rubric(),
                &pages(),
                &[],
                "m",
                NaiveDateTime::default()
            )
            .unwrap(),
            Outcome::Pending("metade ilegível".into())
        );
    }

    #[test]
    fn recurring_issues_need_examples_from_two_attempts() {
        let attempts = vec![
            (AttemptId(10), vec!["Os cidadão participa".to_owned()]),
            (AttemptId(11), vec!["As pessoa vota".to_owned()]),
        ];
        let draft = |examples: Vec<(usize, &str)>| IssueDraft {
            label: "Concordância nominal".into(),
            category: "escrita".into(),
            advice: "Revise o plural".into(),
            examples: examples
                .into_iter()
                .map(|(attempt, quote)| IssueExampleDraft {
                    attempt,
                    quote: quote.into(),
                })
                .collect(),
        };
        let kept = issues(
            vec![
                draft(vec![(1, "Os cidadão"), (2, "As pessoa")]),
                draft(vec![(1, "Os cidadão"), (2, "trecho inventado")]),
                draft(vec![(1, "Os cidadão"), (3, "As pessoa")]),
            ],
            &attempts,
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].examples[1].attempt, AttemptId(11));
    }
}
