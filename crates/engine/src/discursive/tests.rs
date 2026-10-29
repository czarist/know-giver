//! Written answers through the engine. Model results are injected as
//! events; the only network calls go to a closed local port and fail fast.

use std::time::Duration;

use kg_ai::{
    EvaluationDraft, PlanDraft, PlanPrefs, ProposalDraft, RubricDraft, ScoreDraft, SubjectDraft,
    TopicDraft,
};
use kg_pdf::{Edital, EditalText};

use super::images::tests::page_png;
use super::*;
use crate::{AiConfig, AppPaths, Command, RubricSource, WritingKind};

const NOTICE: &str = "EDITAL\n12. DA PROVA DISCURSIVA\n12.1 A prova discursiva consistirá \
    na redação de texto dissertativo de 20 a 30 linhas.\nCONTEÚDO: Português.";

fn engine_with(notice: &str) -> (Engine, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::in_memory(AppPaths::new(dir.path()), AiConfig::default()).unwrap();
    engine.settings.board_images = false;
    // Requests that slip through fail at once instead of reaching a server.
    engine.settings.ai.base_url = "http://127.0.0.1:1/v1".into();
    let draft = PlanDraft {
        title: "Plano".into(),
        exam: "TRF".into(),
        subjects: vec![SubjectDraft {
            name: "Português".into(),
            topics: vec![
                TopicDraft {
                    title: "Crase".into(),
                    difficulty: "medium".into(),
                    week: 1,
                },
                TopicDraft {
                    title: "Concordância".into(),
                    difficulty: "medium".into(),
                    week: 1,
                },
            ],
        }],
    };
    engine
        .events_tx
        .send(Event::PlanGenerated {
            edital: Edital {
                file_name: "edital.pdf".into(),
                pages: 1,
                text: EditalText::from_raw(notice),
            },
            prefs: PlanPrefs {
                weeks: 4,
                hours_per_week: 10,
                role: String::new(),
            },
            result: Ok(draft),
        })
        .unwrap();
    engine.poll();
    engine.take_notices();
    (engine, dir)
}

fn topics(engine: &Engine) -> Vec<TopicId> {
    engine
        .active_plan()
        .unwrap()
        .topics()
        .map(|(_, t)| t.id)
        .collect()
}

fn request() -> ProposalRequest {
    ProposalRequest {
        kind: WritingKind::Essay,
        difficulty: kg_study::Difficulty::Medium,
        topics: Vec::new(),
        manual_rubric: Some("Conteúdo: 6\nLíngua: 4".into()),
        line_limit: Some(25),
        minutes: None,
    }
}

/// Saves a proposal as if the model had written it.
fn proposal(engine: &mut Engine) -> ProposalId {
    let plan = engine.active_plan().unwrap().id;
    let candidates = topics(engine);
    engine
        .events_tx
        .send(Event::Discursive(DiscursiveEvent::Proposal {
            plan,
            request: request(),
            candidates,
            edital: engine.writing_rules().excerpt.clone(),
            model: "quiz-model".into(),
            result: Box::new(Ok(ProposalDraft {
                theme: "Crase no dia a dia".into(),
                statement: "Explique, com exemplos, quando a crase é obrigatória.".into(),
                instructions: "Use a norma-padrão.".into(),
                line_limit: None,
                minutes: None,
                topics: vec![1],
                rubric: RubricDraft::default(),
                expected_points: vec!["fusão da preposição a com o artigo a".into()],
            })),
        }))
        .unwrap();
    engine.poll();
    assert!(matches!(
        engine.take_notices().as_slice(),
        [Notice::ProposalCreated]
    ));
    engine.proposals()[0].id
}

fn open(engine: &Engine) -> &OpenAttempt {
    engine.open_attempt().unwrap()
}

fn status(engine: &Engine, id: AttemptId) -> AttemptStatus {
    engine.store.attempt(id).unwrap().unwrap().status
}

/// Polls until `done` or fails after a few seconds.
fn wait(engine: &mut Engine, done: impl Fn(&Engine) -> bool) {
    for _ in 0..500 {
        engine.poll();
        if done(engine) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out");
}

fn photo(dir: &std::path::Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, page_png(900, 1200, 235)).unwrap();
    path
}

fn reading(page: u32, text: &str) -> (TranscriptPage, Vec<PageIssue>) {
    (
        TranscriptPage {
            page,
            lines: text.lines().map(str::to_owned).collect(),
            uncertain: Vec::new(),
            legibility: Legibility::Good,
            warnings: Vec::new(),
            retake: None,
        },
        Vec::new(),
    )
}

const ANSWER: &str = "A crase indica a fusão da preposição a com o artigo feminino a.\n\
    Ela é obrigatória antes de palavras femininas que pedem artigo,\n\
    como em vou à escola, e proibida antes de verbos e de palavras masculinas.";

/// An attempt whose transcript waits for the student's check.
fn attempt_in_review(engine: &mut Engine, proposal: ProposalId) -> AttemptId {
    engine.handle(Command::StartAttempt(proposal)).unwrap();
    let id = open(engine).attempt.id;
    let run = engine
        .store
        .begin_run(id, AttemptStatus::Transcribing, now())
        .unwrap()
        .unwrap();
    engine
        .events_tx
        .send(Event::Discursive(DiscursiveEvent::Transcribed {
            attempt: id,
            run,
            model: "vision".into(),
            result: Ok(vec![reading(1, ANSWER)]),
        }))
        .unwrap();
    engine.poll();
    assert_eq!(status(engine, id), AttemptStatus::Review);
    id
}

#[test]
fn the_area_follows_what_the_notice_says() {
    let (mut engine, _dir) = engine_with("EDITAL\nCONTEÚDO: Redação Oficial; crase.");
    assert!(!engine.writing_rules().has_written_test());
    assert!(matches!(
        engine.handle(Command::GenerateProposal(request())),
        Err(EngineError::NoWrittenTest)
    ));

    let (mut engine, _dir) = engine_with(NOTICE);
    assert_eq!(
        engine.writing_rules().kinds,
        vec![WritingKind::Essay, WritingKind::Technical]
    );
    let mut case = request();
    case.kind = WritingKind::CaseStudy;
    assert!(matches!(
        engine.handle(Command::GenerateProposal(case)),
        Err(EngineError::NoWrittenTest)
    ));
    let mut bad = request();
    bad.manual_rubric = Some("sem pontos".into());
    assert!(matches!(
        engine.handle(Command::GenerateProposal(bad)),
        Err(EngineError::Rubric(_))
    ));

    let id = proposal(&mut engine);
    let saved = &engine.proposals()[0];
    assert_eq!(saved.id, id);
    assert_eq!(saved.rubric.source, RubricSource::Manual);
    assert_eq!(saved.line_limit, Some(25));
    assert_eq!(saved.model, "quiz-model");
    assert!(
        engine.expected_points(saved).is_none(),
        "hidden before a grade"
    );
}

#[test]
fn pages_are_added_in_order_turned_moved_and_limited() {
    let (mut engine, dir) = engine_with(NOTICE);
    let proposal = proposal(&mut engine);
    engine.handle(Command::StartAttempt(proposal)).unwrap();
    let files: Vec<PathBuf> = ["a.png", "b.png", "c.jpg"]
        .iter()
        .map(|n| photo(dir.path(), n))
        .collect();
    let fake = dir.path().join("fake.jpg");
    std::fs::write(&fake, b"not a photo").unwrap();
    engine
        .handle(Command::AddPages(vec![
            files[0].clone(),
            fake,
            files[1].clone(),
        ]))
        .unwrap();
    wait(&mut engine, |e| e.pages_in_progress() == 0);
    assert!(matches!(
        engine.take_notices().as_slice(),
        [Notice::Error(EngineError::Image(ImageProblem::Unsupported))]
    ));
    let pages = open(&engine).pages.clone();
    assert_eq!(pages.len(), 2);
    assert!(pages[0].original_path.ends_with(".png"));
    assert!(engine.page_image(pages[0].id).is_some());

    engine
        .handle(Command::MovePage {
            page: pages[1].id,
            up: true,
        })
        .unwrap();
    let order: Vec<PageId> = open(&engine).pages.iter().map(|p| p.id).collect();
    assert_eq!(order, vec![pages[1].id, pages[0].id]);

    let (version, _) = engine.page_image(pages[0].id).unwrap();
    engine.handle(Command::RotatePage(pages[0].id)).unwrap();
    wait(&mut engine, |e| e.pages_in_progress() == 0);
    let turned = open(&engine)
        .pages
        .iter()
        .find(|p| p.id == pages[0].id)
        .unwrap()
        .clone();
    assert_eq!(
        (turned.rotation, turned.width, turned.height),
        (1, 1200, 900)
    );
    assert!(engine.page_image(pages[0].id).unwrap().0 > version);
    assert!(!std::path::Path::new(&pages[0].processed_path).exists());

    engine
        .handle(Command::ReplacePage {
            page: pages[1].id,
            path: files[2].clone(),
        })
        .unwrap();
    wait(&mut engine, |e| e.pages_in_progress() == 0);
    assert!(!std::path::Path::new(&pages[1].original_path).exists());
    assert_eq!(open(&engine).pages[0].id, pages[1].id, "kept its place");

    engine.handle(Command::RemovePage(pages[0].id)).unwrap();
    assert_eq!(open(&engine).pages.len(), 1);

    let many = vec![files[0].clone(); MAX_PAGES];
    assert!(matches!(
        engine.handle(Command::AddPages(many)),
        Err(EngineError::TooManyPages)
    ));
}

#[test]
fn photos_never_go_to_a_text_model() {
    let (mut engine, dir) = engine_with(NOTICE);
    let proposal = proposal(&mut engine);
    engine.handle(Command::StartAttempt(proposal)).unwrap();
    engine
        .handle(Command::AddPages(vec![photo(dir.path(), "a.png")]))
        .unwrap();
    wait(&mut engine, |e| e.pages_in_progress() == 0);
    assert!(engine.settings.ai.models.vision.is_empty());
    assert!(matches!(
        engine.handle(Command::SubmitPages),
        Err(EngineError::Ai(AiError::NoVisionModel))
    ));
    assert!(matches!(
        engine.handle(Command::CheckVision),
        Err(EngineError::Ai(AiError::NoVisionModel))
    ));
    let id = open(&engine).attempt.id;
    assert_eq!(status(&engine, id), AttemptStatus::Draft);
    assert_eq!(engine.store.attempt(id).unwrap().unwrap().run, 0);
}

#[test]
fn results_only_reach_their_own_attempt_and_run() {
    let (mut engine, _dir) = engine_with(NOTICE);
    let proposal = proposal(&mut engine);
    engine.handle(Command::StartAttempt(proposal)).unwrap();
    let first = open(&engine).attempt.id;
    engine.handle(Command::StartAttempt(proposal)).unwrap();
    let second = open(&engine).attempt.id;
    let run_first = engine
        .store
        .begin_run(first, AttemptStatus::Transcribing, now())
        .unwrap()
        .unwrap();
    let run_second = engine
        .store
        .begin_run(second, AttemptStatus::Transcribing, now())
        .unwrap()
        .unwrap();
    let send = |engine: &mut Engine, attempt, run, text: &str| {
        engine
            .events_tx
            .send(Event::Discursive(DiscursiveEvent::Transcribed {
                attempt,
                run,
                model: "vision".into(),
                result: Ok(vec![reading(1, text)]),
            }))
            .unwrap();
        engine.poll();
    };
    // A result from an older run of the first attempt is dropped.
    send(&mut engine, first, run_first + 1, "texto velho");
    assert_eq!(status(&engine, first), AttemptStatus::Transcribing);
    assert!(engine.store.transcript(first).unwrap().is_none());
    send(&mut engine, second, run_second, "texto da segunda");
    assert_eq!(status(&engine, second), AttemptStatus::Review);
    assert_eq!(status(&engine, first), AttemptStatus::Transcribing);
    assert!(engine.store.transcript(first).unwrap().is_none());
    // A grade for an attempt that is not being graded is not applied.
    engine
        .events_tx
        .send(Event::Discursive(DiscursiveEvent::Evaluated {
            attempt: second,
            run: run_second,
            result: Err(AiError::InvalidResponse("x".into()).into()),
        }))
        .unwrap();
    engine.poll();
    assert_eq!(status(&engine, second), AttemptStatus::Review);
}

#[test]
fn a_page_to_photograph_again_gets_no_grade() {
    let (mut engine, _dir) = engine_with(NOTICE);
    let proposal = proposal(&mut engine);
    engine.handle(Command::StartAttempt(proposal)).unwrap();
    let id = open(&engine).attempt.id;
    let run = engine
        .store
        .begin_run(id, AttemptStatus::Transcribing, now())
        .unwrap()
        .unwrap();
    let mut cut = reading(1, "A crase [ilegível]");
    cut.0.retake = Some("texto cortado na borda direita".into());
    engine
        .events_tx
        .send(Event::Discursive(DiscursiveEvent::Transcribed {
            attempt: id,
            run,
            model: "vision".into(),
            result: Ok(vec![cut]),
        }))
        .unwrap();
    engine.poll();
    let attempt = &open(&engine).attempt;
    assert_eq!(attempt.status, AttemptStatus::Retake);
    assert!(attempt.note.contains("cortado"));
    assert!(attempt.status.pages_editable(), "pages can be replaced");
    assert!(engine.store.evaluation(id).unwrap().is_none());
}

#[test]
fn unsure_reading_is_left_pending_and_rewrites_are_new_attempts() {
    let (mut engine, _dir) = engine_with(NOTICE);
    let proposal = proposal(&mut engine);
    let id = attempt_in_review(&mut engine, proposal);

    // Rewriting the answer is not checking the transcript.
    let rewritten =
        vec!["Um texto completamente diferente escrito agora no aplicativo.".to_owned()];
    assert!(matches!(
        engine.handle(Command::ConfirmTranscript(rewritten.clone())),
        Err(EngineError::LooksLikeRewrite)
    ));
    assert_eq!(status(&engine, id), AttemptStatus::Review);

    // Too much could not be read: pending, with no request to the model.
    let unsure = vec![
        ANSWER
            .replace("fusão", "[ilegível]")
            .replace("obrigatória", "[?obrigatória?]")
            .replace("escola", "[ilegível]")
            .replace("proibida", "[?proibida?]")
            .replace("preposição", "[ilegível]")
            .replace("artigo", "[?artigo?]"),
    ];
    engine
        .handle(Command::ConfirmTranscript(unsure.clone()))
        .unwrap();
    let attempt = engine.store.attempt(id).unwrap().unwrap();
    assert_eq!(attempt.status, AttemptStatus::Pending);
    assert_eq!(attempt.note, NOTE_UNREADABLE);
    assert_eq!(attempt.run, 1, "no evaluation run started");
    let saved = engine.store.transcript(id).unwrap().unwrap();
    assert_eq!(
        saved.original.pages[0].lines.join("\n"),
        ANSWER,
        "original kept"
    );
    assert_eq!(saved.confirmed.unwrap().pages, unsure);

    engine.handle(Command::ReopenTranscript).unwrap();
    assert_eq!(status(&engine, id), AttemptStatus::Review);
    engine.handle(Command::SaveRewrite(rewritten)).unwrap();
    let rewrite = open(&engine).attempt.clone();
    assert_eq!(rewrite.origin, AttemptOrigin::Rewrite);
    assert_eq!(rewrite.parent, Some(id));
    assert_eq!(rewrite.number, 2);
    assert_eq!(
        status(&engine, id),
        AttemptStatus::Review,
        "original untouched"
    );
}

#[test]
fn grading_runs_once_fails_safely_and_can_be_retried() {
    let (mut engine, _dir) = engine_with(NOTICE);
    let proposal = proposal(&mut engine);
    let id = attempt_in_review(&mut engine, proposal);
    let fixed = vec![ANSWER.replace("fusão", "junção")];
    engine
        .handle(Command::ConfirmTranscript(fixed.clone()))
        .unwrap();
    assert_eq!(status(&engine, id), AttemptStatus::Evaluating);
    let edits = engine
        .store
        .transcript(id)
        .unwrap()
        .unwrap()
        .confirmed
        .unwrap()
        .edits;
    assert_eq!(edits.len(), 1);
    // A second click while grading starts nothing.
    assert!(matches!(
        engine.handle(Command::ConfirmTranscript(fixed)),
        Err(EngineError::AttemptLocked)
    ));
    assert_eq!(engine.store.attempt(id).unwrap().unwrap().run, 2);

    // The server is unreachable: the attempt fails and keeps its text.
    wait(&mut engine, |e| status(e, id) == AttemptStatus::Failed);
    let failed = engine.store.attempt(id).unwrap().unwrap();
    assert_eq!(failed.failed_stage, Some(Stage::Evaluation));
    engine.handle(Command::RetryAttempt).unwrap();
    let run = engine.store.attempt(id).unwrap().unwrap().run;
    assert_eq!(run, 3);
    assert_eq!(status(&engine, id), AttemptStatus::Evaluating);

    // The model's answer for this run arrives (validated in code).
    let rubric = engine.proposals()[0].rubric.clone();
    let draft = EvaluationDraft {
        criteria: vec![
            ScoreDraft {
                key: "c1".into(),
                score: Some(5.0),
                justification: "Correto.".into(),
                evidence: Vec::new(),
            },
            ScoreDraft {
                key: "c2".into(),
                score: Some(3.5),
                justification: "Boa norma.".into(),
                evidence: Vec::new(),
            },
        ],
        ..EvaluationDraft::default()
    };
    let pages = vec![ANSWER.to_owned()];
    let outcome = validate::evaluation(draft, &rubric, &pages, &[], "m", now()).unwrap();
    // A late failure from the earlier run is ignored.
    for (run, result) in [
        (run - 1, Err(AiError::InvalidResponse("late".into()).into())),
        (run, Ok(outcome)),
    ] {
        engine
            .events_tx
            .send(Event::Discursive(DiscursiveEvent::Evaluated {
                attempt: id,
                run,
                result,
            }))
            .unwrap();
    }
    // The real worker of this run may also answer (connection refused);
    // whichever comes first for the run decides, so wait for a settled state.
    wait(&mut engine, |e| !status(e, id).is_processing());
    if status(&engine, id) == AttemptStatus::Evaluated {
        let attempt = engine.store.attempt(id).unwrap().unwrap();
        assert_eq!(attempt.score, Some((8.5, 10.0)));
        assert!(engine.expected_points(&engine.proposals()[0]).is_some());
        assert_eq!(engine.writing_evolution().len(), 1);
    }
}

#[test]
fn an_invalid_model_answer_fails_the_step() {
    let (mut engine, _dir) = engine_with(NOTICE);
    let proposal = proposal(&mut engine);
    let id = attempt_in_review(&mut engine, proposal);
    engine
        .store
        .set_attempt_status(id, AttemptStatus::Review, None, "", now())
        .unwrap();
    let run = engine
        .store
        .begin_run(id, AttemptStatus::Evaluating, now())
        .unwrap()
        .unwrap();
    engine
        .events_tx
        .send(Event::Discursive(DiscursiveEvent::Evaluated {
            attempt: id,
            run,
            result: Err(AiError::InvalidResponse("score of c1 out of range".into()).into()),
        }))
        .unwrap();
    engine.poll();
    let attempt = engine.store.attempt(id).unwrap().unwrap();
    assert_eq!(attempt.status, AttemptStatus::Failed);
    assert_eq!(attempt.failed_stage, Some(Stage::Evaluation));
    assert!(engine.store.evaluation(id).unwrap().is_none());
}

#[test]
fn interrupted_work_is_marked_failed_on_start() {
    let (mut engine, _dir) = engine_with(NOTICE);
    let proposal = proposal(&mut engine);
    engine.handle(Command::StartAttempt(proposal)).unwrap();
    let id = open(&engine).attempt.id;
    engine
        .store
        .begin_run(id, AttemptStatus::Transcribing, now())
        .unwrap();
    engine.recover_writing();
    let attempt = engine.store.attempt(id).unwrap().unwrap();
    assert_eq!(attempt.status, AttemptStatus::Failed);
    assert_eq!(attempt.note, NOTE_INTERRUPTED);
}

#[test]
fn deleting_an_attempt_removes_its_photos() {
    let (mut engine, dir) = engine_with(NOTICE);
    let proposal = proposal(&mut engine);
    engine.handle(Command::StartAttempt(proposal)).unwrap();
    engine
        .handle(Command::AddPages(vec![photo(dir.path(), "a.png")]))
        .unwrap();
    wait(&mut engine, |e| e.pages_in_progress() == 0);
    let id = open(&engine).attempt.id;
    let folder = images::attempt_dir(&engine.paths.data_dir, id);
    assert!(folder.exists());
    engine.handle(Command::DeleteAttempt(id)).unwrap();
    assert!(!folder.exists());
    assert!(engine.open_attempt().is_none());
    assert!(engine.writing_attempts().is_empty());
}

#[test]
fn vision_host_is_named_before_photos_are_sent() {
    let (mut engine, _dir) = engine_with(NOTICE);
    assert!(engine.vision_target().is_none());
    engine.settings.ai.models.vision = "gpt-vision".into();
    assert_eq!(
        engine.vision_target(),
        Some(("gpt-vision".into(), "127.0.0.1:1".into()))
    );
}

#[test]
fn reading_the_notice_again_keeps_the_plan_and_opens_the_area() {
    let (mut engine, _dir) = engine_with("EDITAL\nCONTEÚDO: Português.");
    assert!(!engine.writing_rules().has_written_test());
    let plan = engine.active_plan().unwrap().id;
    let before = topics(&engine);
    let first = before[0];
    engine
        .handle(Command::SetTopicStatus(first, kg_study::TopicStatus::Done))
        .unwrap();
    engine
        .events_tx
        .send(Event::EditalRevalidated {
            plan,
            result: Ok(Edital {
                file_name: "edital-retificado.pdf".into(),
                pages: 2,
                text: EditalText::from_raw(NOTICE),
            }),
        })
        .unwrap();
    engine.revalidating = Some(plan);
    engine.take_notices();
    engine.poll();
    assert!(engine.revalidating().is_none());
    assert!(matches!(
        engine.take_notices().as_slice(),
        [Notice::EditalRevalidated { written_test: true }]
    ));
    assert!(engine.writing_rules().has_written_test());
    assert_eq!(topics(&engine), before, "topics kept");
    let kept = engine
        .active_plan()
        .unwrap()
        .find_topic(first)
        .unwrap()
        .1
        .status;
    assert_eq!(kept, kg_study::TopicStatus::Done, "progress kept");
    assert!(
        engine
            .store
            .edital_text(plan)
            .unwrap()
            .contains("PROVA DISCURSIVA")
    );
}
