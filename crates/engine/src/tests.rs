//! Orchestration tests. Worker results are injected as events, so no network
//! or audio device is needed.

use kg_ai::{PlanDraft, PlanPrefs, QuestionDraft, ReviewFeedback, SubjectDraft, TopicDraft};
use kg_pdf::{Edital, EditalText};

use crate::lesson::LessonEvent;
use crate::practice::PracticeEvent;
use crate::*;

fn engine() -> (Engine, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::in_memory(AppPaths::new(dir.path()), AiConfig::default()).unwrap();
    // Tests never reach Wikipedia.
    engine.settings.board_images = false;
    (engine, dir)
}

fn edital() -> Edital {
    Edital {
        file_name: "edital.pdf".into(),
        pages: 3,
        text: EditalText::from_raw("EDITAL\nCONTEÚDO PROGRAMÁTICO\nPortuguês: crase."),
    }
}

fn draft() -> PlanDraft {
    let topic = |title: &str, week| TopicDraft {
        title: title.into(),
        difficulty: "medium".into(),
        week,
    };
    PlanDraft {
        title: "Plano TRF".into(),
        exam: "TRF – Técnico".into(),
        subjects: vec![
            SubjectDraft {
                name: "Português".into(),
                topics: vec![topic("Crase", 1), topic("Regência", 2)],
            },
            SubjectDraft {
                name: "Direito Constitucional".into(),
                topics: vec![topic("Direitos fundamentais", 1)],
            },
        ],
    }
}

fn prefs() -> PlanPrefs {
    PlanPrefs {
        weeks: 6,
        hours_per_week: 10,
        role: String::new(),
    }
}

/// Engine with one generated plan.
fn engine_with_plan() -> (Engine, tempfile::TempDir) {
    let (mut engine, dir) = engine();
    engine
        .events_tx
        .send(Event::PlanGenerated {
            edital: edital(),
            prefs: prefs(),
            result: Ok(draft()),
        })
        .unwrap();
    engine.poll();
    (engine, dir)
}

fn first_topic(engine: &Engine) -> TopicId {
    engine.active_plan().unwrap().subjects[0].topics[0].id
}

fn topic(engine: &Engine, id: TopicId) -> Topic {
    engine
        .active_plan()
        .unwrap()
        .find_topic(id)
        .unwrap()
        .1
        .clone()
}

#[test]
fn generated_plan_is_stored_and_selected() {
    let (mut engine, _dir) = engine_with_plan();
    let notices = engine.take_notices();
    assert!(notices.iter().any(|n| matches!(n, Notice::PlanCreated)));
    let plan = engine.active_plan().unwrap();
    assert_eq!(plan.title, "Plano TRF");
    assert_eq!(plan.topic_count(), 3);
    assert_eq!(engine.plans().len(), 1);
    assert_eq!(engine.import_state(), &ImportState::Empty);
}

#[test]
fn handout_edital_follows_the_selected_plan() {
    let (mut engine, _dir) = engine_with_plan();
    let first = engine.active_plan().unwrap().id;
    assert!(
        engine
            .handout_edital()
            .unwrap()
            .contains("Português: crase.")
    );
    let mut second_edital = edital();
    second_edital.text = EditalText::from_raw("EDITAL B\nMatemática: porcentagem.");
    engine
        .on_plan_generated(second_edital, &prefs(), Ok(draft()))
        .unwrap();
    assert!(
        engine
            .handout_edital()
            .unwrap()
            .contains("Matemática: porcentagem.")
    );
    assert!(
        !engine
            .handout_edital()
            .unwrap()
            .contains("Português: crase.")
    );
    engine.select_plan(first).unwrap();
    assert!(
        engine
            .handout_edital()
            .unwrap()
            .contains("Português: crase.")
    );
}

#[test]
fn generate_requires_imported_edital() {
    let (mut engine, _dir) = engine();
    assert!(matches!(
        engine.handle(Command::GeneratePlan(prefs())),
        Err(EngineError::NoEdital)
    ));
}

#[test]
fn ai_failure_becomes_error_notice() {
    let (mut engine, _dir) = engine();
    engine
        .events_tx
        .send(Event::PlanGenerated {
            edital: edital(),
            prefs: prefs(),
            result: Err(AiError::RateLimited),
        })
        .unwrap();
    engine.poll();
    let notices = engine.take_notices();
    assert!(matches!(
        notices.as_slice(),
        [Notice::Error(EngineError::Ai(AiError::RateLimited))]
    ));
    assert!(engine.active_plan().is_none());
}

#[test]
fn quiz_flow_records_attempt_and_schedules_review() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartQuiz(id)).unwrap();
    let session = engine.quiz().unwrap().id;
    let question = |correct| QuestionDraft {
        prompt: "Qual?".into(),
        options: vec!["a".into(), "b".into(), "c".into(), "d".into()],
        correct,
        explanation: "Porque sim.".into(),
    };
    engine
        .events_tx
        .send(Event::Practice(PracticeEvent::QuizReady {
            session,
            result: Ok(vec![question(1), question(2)]),
        }))
        .unwrap();
    engine.poll();

    // Options are shuffled, so answer by the current position of the right one.
    let right = |engine: &Engine| engine.quiz().unwrap().current().unwrap().correct;
    let first = right(&engine);
    engine.handle(Command::AnswerQuiz(first)).unwrap();
    // A second answer to the same question is ignored.
    engine.handle(Command::AnswerQuiz((first + 1) % 4)).unwrap();
    engine.handle(Command::NextQuestion).unwrap();
    let second = right(&engine);
    engine
        .handle(Command::AnswerQuiz((second + 1) % 4))
        .unwrap();
    engine.handle(Command::NextQuestion).unwrap();

    assert_eq!(
        engine.quiz().unwrap().step,
        QuizStep::Finished {
            correct: 1,
            total: 2
        }
    );
    let stats = engine.quiz_stats();
    assert_eq!((stats.attempts, stats.correct, stats.total), (1, 1, 2));
    let t = topic(&engine, id);
    assert_eq!(t.status, TopicStatus::Reviewing);
    assert!(t.review.due.is_some());
}

#[test]
fn invalid_quiz_from_model_is_reported() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartQuiz(id)).unwrap();
    let session = engine.quiz().unwrap().id;
    engine
        .events_tx
        .send(Event::Practice(PracticeEvent::QuizReady {
            session,
            result: Ok(vec![QuestionDraft {
                prompt: "Qual?".into(),
                options: vec!["a".into()],
                correct: 3,
                explanation: String::new(),
            }]),
        }))
        .unwrap();
    engine.poll();
    assert!(engine.quiz().is_none());
    assert!(
        engine
            .take_notices()
            .iter()
            .any(|n| matches!(n, Notice::Error(EngineError::Study(_))))
    );
}

#[test]
fn review_flow_uses_feedback_and_saves_grade() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartReview(id)).unwrap();
    let session = engine.review().unwrap().id;
    engine
        .events_tx
        .send(Event::Practice(PracticeEvent::ReviewQuestion {
            session,
            result: Ok("Quando usar crase antes de horas?".into()),
        }))
        .unwrap();
    engine.poll();
    engine
        .handle(Command::SubmitReviewAnswer(
            "Antes de horas determinadas.".into(),
        ))
        .unwrap();
    assert!(engine.review().unwrap().is_loading());
    engine
        .events_tx
        .send(Event::Practice(PracticeEvent::ReviewGraded {
            session,
            result: Ok(ReviewFeedback {
                score: 4,
                feedback: "Correto.".into(),
            }),
        }))
        .unwrap();
    engine.poll();
    let ReviewStep::Graded { suggested, .. } = engine.review().unwrap().step.clone() else {
        panic!("expected graded step");
    };
    assert_eq!(suggested, ReviewGrade::Good);

    engine.handle(Command::GradeReview(suggested)).unwrap();
    assert!(engine.review().is_none());
    let t = topic(&engine, id);
    assert_eq!(t.review.repetitions, 1);
    assert!(
        engine
            .take_notices()
            .iter()
            .any(|n| matches!(n, Notice::ReviewSaved { next_due: Some(_) }))
    );
}

#[test]
fn stale_results_are_ignored() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartReview(id)).unwrap();
    let old = engine.review().unwrap().id;
    engine.handle(Command::CloseReview).unwrap();
    engine.handle(Command::StartReview(id)).unwrap();
    engine
        .events_tx
        .send(Event::Practice(PracticeEvent::ReviewQuestion {
            session: old,
            result: Ok("velha".into()),
        }))
        .unwrap();
    engine.poll();
    assert_eq!(engine.review().unwrap().step, ReviewStep::LoadingQuestion);
}

#[test]
fn topic_status_and_plan_deletion() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine
        .handle(Command::SetTopicStatus(id, TopicStatus::Done))
        .unwrap();
    assert_eq!(topic(&engine, id).status, TopicStatus::Done);
    assert_eq!(engine.plans()[0].done_count, 1);

    let plan = engine.active_plan().unwrap().id;
    engine.handle(Command::DeletePlan(plan)).unwrap();
    assert!(engine.active_plan().is_none());
    assert!(engine.plans().is_empty());
}

#[test]
fn settings_are_saved() {
    let (mut engine, _dir) = engine();
    let mut settings = engine.settings().clone();
    settings.quiz_size = 7;
    settings.voice = Voice::Piper(PiperVoice::Cadu);
    engine.handle(Command::SaveSettings(settings)).unwrap();
    assert_eq!(engine.settings().quiz_size, 7);
    assert!(
        engine
            .take_notices()
            .iter()
            .any(|n| matches!(n, Notice::SettingsSaved))
    );
}

#[test]
fn lesson_without_speech_model_reports_missing_model() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartLesson(id)).unwrap();
    assert_eq!(topic(&engine, id).status, TopicStatus::Studying);
    assert!(matches!(
        engine.handle(Command::StartListening),
        Err(EngineError::SpeechModelMissing(ModelId::Parakeet))
    ));
    // Ending a lesson without any student turn does not count as studied.
    engine.handle(Command::EndLesson).unwrap();
    assert_eq!(engine.lesson_count(), 0);
}

#[test]
fn export_writes_file() {
    let (mut engine, dir) = engine();
    let path = dir.path().join("progresso.md");
    engine
        .handle(Command::ExportProgress {
            path: path.clone(),
            content: "# Progresso".into(),
        })
        .unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "# Progresso");
}

#[test]
fn board_exam_feeds_quiz_without_model_calls() {
    let (mut engine, _dir) = engine_with_plan();
    engine.take_notices();
    let plan = engine.active_plan().unwrap().id;
    let topic = first_topic(&engine);
    let question = |n: u32, topic_id, relevance| kg_study::NewBoardQuestion {
        number: Some(n),
        prompt: format!("Item {n} sobre crase."),
        options: vec!["Certo".into(), "Errado".into()],
        correct: Some((n % 2) as usize),
        topic_id,
        relevance,
        reason: String::new(),
    };
    let mut questions: Vec<_> = (1..=5)
        .map(|n| question(n, Some(topic), Relevance::Relevant))
        .collect();
    questions.push(question(6, None, Relevance::OutOfScope));
    engine
        .events_tx
        .send(Event::BoardImported(Ok(board::BoardImport {
            plan,
            kind: DocumentKind::Exam,
            file_name: "prova.pdf".into(),
            text: "texto".into(),
            questions,
            excerpts: vec![],
        })))
        .unwrap();
    engine.poll();

    assert!(engine.take_notices().iter().any(|n| matches!(
        n,
        Notice::BoardImported {
            used: 5,
            set_aside: 1,
            ..
        }
    )));
    assert_eq!(engine.board_documents().len(), 1);
    assert_eq!(engine.board_emphasis(topic), 5);

    // Five real questions fill a five-question quiz: it is ready at once.
    engine.handle(Command::StartQuiz(topic)).unwrap();
    let quiz = engine.quiz().unwrap();
    assert!(matches!(quiz.step, QuizStep::Question { index: 0, .. }));
    assert_eq!(quiz.questions().len(), 5);
    assert!(quiz.questions().iter().all(|q| q.from_board));

    // Moving the set-aside question into use counts it for the topic.
    let set_aside = engine
        .board_questions()
        .iter()
        .find(|q| q.relevance == Relevance::OutOfScope)
        .unwrap()
        .id;
    engine
        .handle(Command::SetBoardItem {
            item: BoardItem::Question(set_aside),
            relevance: Relevance::Relevant,
            topic: Some(topic),
        })
        .unwrap();
    assert_eq!(engine.board_emphasis(topic), 6);
    assert!(
        engine
            .board_reference(topic)
            .contains("Item 1 sobre crase.")
    );
}

#[test]
fn quiz_counts_toward_streak_and_milestones_show_once() {
    let (mut engine, _dir) = engine_with_plan();
    assert_eq!(engine.streak(), 0);
    assert!(!engine.studied_today());
    let id = first_topic(&engine);

    // Marking 1 of 3 topics done crosses 25%: celebrated once.
    engine
        .handle(Command::SetTopicStatus(id, TopicStatus::Done))
        .unwrap();
    engine
        .handle(Command::SetTopicStatus(id, TopicStatus::Reviewing))
        .unwrap();
    engine
        .handle(Command::SetTopicStatus(id, TopicStatus::Done))
        .unwrap();
    let progress_notices = engine
        .take_notices()
        .into_iter()
        .filter(|n| matches!(n, Notice::Milestone(Milestone::Progress(25))))
        .count();
    assert_eq!(progress_notices, 1);

    engine.handle(Command::StartReview(id)).unwrap();
    engine
        .handle(Command::GradeReview(ReviewGrade::Good))
        .unwrap();
    assert_eq!(engine.streak(), 1);
    assert!(engine.studied_today());
}

#[test]
fn onboarding_only_without_plans() {
    let (mut engine, _dir) = engine();
    assert!(engine.needs_onboarding());
    engine.handle(Command::FinishOnboarding).unwrap();
    assert!(!engine.needs_onboarding());
    let (with_plan, _dir) = engine_with_plan();
    assert!(!with_plan.needs_onboarding());
}

#[test]
fn streamed_sentences_grow_one_caption_and_the_reply_replaces_it() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartLesson(id)).unwrap();
    // Streamed sentences belong to the reply being received.
    let session = engine.lesson().unwrap().reply_id();
    for text in ["Olá!", "Hoje vamos ver crase."] {
        engine
            .events_tx
            .send(Event::Lesson(LessonEvent::Sentence {
                session,
                text: text.into(),
            }))
            .unwrap();
    }
    engine.poll();
    let lesson = engine.lesson().unwrap();
    assert_eq!(lesson.turns.len(), 1);
    assert_eq!(lesson.turns[0].text, "Olá! Hoje vamos ver crase.");
    // No voice installed here: the tutor is still "thinking" until the end.
    assert_eq!(lesson.phase, LessonPhase::Thinking);

    engine
        .events_tx
        .send(Event::Lesson(LessonEvent::Reply {
            session,
            result: Ok(kg_ai::TutorReply {
                speech: "Olá! Hoje vamos ver crase. Pronto?".into(),
                board: vec!["Crase = a + a".into()],
                ..kg_ai::TutorReply::default()
            }),
        }))
        .unwrap();
    engine.poll();
    let lesson = engine.lesson().unwrap();
    assert_eq!(lesson.turns.len(), 1);
    assert_eq!(lesson.turns[0].text, "Olá! Hoje vamos ver crase. Pronto?");
    assert_eq!(lesson.board, vec!["Crase = a + a"]);
    assert_eq!(lesson.phase, LessonPhase::Idle);
}

#[test]
fn a_spoken_reply_shows_its_full_text_only_after_the_voice() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartLesson(id)).unwrap();
    let session = engine.lesson().unwrap().reply_id();
    let speech = engine.lesson.as_mut().unwrap().fake_speech();
    // The speech thread sends a caption when a sentence starts playing.
    engine
        .events_tx
        .send(Event::Lesson(LessonEvent::Sentence {
            session,
            text: "Olá!".into(),
        }))
        .unwrap();
    engine
        .events_tx
        .send(Event::Lesson(LessonEvent::Reply {
            session,
            result: Ok(kg_ai::TutorReply {
                speech: "Olá! Hoje vamos ver crase.".into(),
                ..kg_ai::TutorReply::default()
            }),
        }))
        .unwrap();
    engine.poll();
    let lesson = engine.lesson().unwrap();
    // Still speaking: only what was heard is on screen.
    assert_eq!(lesson.phase, LessonPhase::Speaking);
    assert_eq!(lesson.turns.len(), 1);
    assert_eq!(lesson.turns[0].text, "Olá!");

    engine
        .events_tx
        .send(Event::Lesson(LessonEvent::Sentence {
            session,
            text: "Hoje vamos ver crase.".into(),
        }))
        .unwrap();
    engine.poll();
    assert_eq!(
        engine.lesson().unwrap().turns[0].text,
        "Olá! Hoje vamos ver crase."
    );

    speech.store(false, std::sync::atomic::Ordering::Relaxed);
    engine.poll();
    let lesson = engine.lesson().unwrap();
    assert_eq!(lesson.phase, LessonPhase::Idle);
    assert_eq!(lesson.turns.len(), 1);
    assert_eq!(lesson.turns[0].text, "Olá! Hoje vamos ver crase.");
}

#[test]
fn board_opens_with_a_topic_picture_and_asks_for_tutor_pictures() {
    let (mut engine, _dir) = engine_with_plan();
    engine.settings.board_images = true;
    let id = first_topic(&engine);
    let title = topic(&engine, id).title;
    // Pretend the topic's picture was already looked up, so no request leaves.
    engine.on_illustration(crate::illustration_key(&title), None);
    engine.handle(Command::StartLesson(id)).unwrap();
    let lesson = engine.lesson().unwrap();
    assert_eq!(lesson.board, vec![format!("IMAGEM: {title}")]);
    assert_eq!(
        engine.illustration(&title),
        Some(&crate::Illustration::Missing)
    );
}

#[test]
fn lesson_follows_the_tutor_plan_to_the_end() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartLesson(id)).unwrap();
    let lesson = engine.lesson().unwrap();
    assert!(lesson.outline.is_empty());
    assert_eq!(lesson.total_steps(), 1);

    engine.simulate_tutor_text(
        "Oi! Vamos lá.
ROTEIRO: O que é | Quando usar | Exceções",
    );
    let lesson = engine.lesson().unwrap();
    assert_eq!(lesson.outline, ["O que é", "Quando usar", "Exceções"]);
    assert_eq!((lesson.step, lesson.total_steps()), (0, 4));

    engine.simulate_tutor_text(
        "Isso.
ETAPA: 3",
    );
    assert_eq!(engine.lesson().unwrap().step, 2);
    // A later plan or an older step number never undoes progress.
    engine.simulate_tutor_text(
        "Voltando.
ETAPA: 1
ROTEIRO: Outra | Coisa",
    );
    let lesson = engine.lesson().unwrap();
    assert_eq!(lesson.step, 2);
    assert_eq!(lesson.outline.len(), 3);
    assert!(!lesson.finished);

    engine.simulate_tutor_text(
        "Parabéns!
FIM",
    );
    let lesson = engine.lesson().unwrap();
    assert!(lesson.finished);
    assert_eq!(lesson.step, 3);
}

#[test]
fn a_finished_lesson_counts_without_pressing_end_and_only_once() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartLesson(id)).unwrap();
    engine.simulate_tutor_text(
        "Parabéns, terminamos!
FIM",
    );
    assert!(topic(&engine, id).lesson_taken());
    assert_eq!(engine.lesson_count(), 1);
    // Ending it afterwards does not count it twice.
    engine.handle(Command::EndLesson).unwrap();
    assert_eq!(engine.lesson_count(), 1);

    // Taking it again keeps it taken.
    engine.handle(Command::StartLesson(id)).unwrap();
    engine.handle(Command::EndLesson).unwrap();
    assert!(topic(&engine, id).lesson_taken());
}

fn prepare_request() -> PrepareRequest {
    PrepareRequest {
        file_prefix: "apostila".into(),
        whole_plan: "Plano completo".into(),
        generated: "Gerado hoje".into(),
        lang: "pt".into(),
        labels: HandoutLabels::default(),
    }
}

#[test]
fn preparation_asked_during_plan_generation_waits_for_the_plan() {
    let (mut engine, _dir) = engine();
    engine.generating_since = Some(std::time::Instant::now());
    engine
        .handle(Command::PrepareContent(prepare_request()))
        .unwrap();
    assert!(engine.prepare_job().is_none());
    engine
        .events_tx
        .send(Event::PlanGenerated {
            edital: edital(),
            prefs: prefs(),
            result: Ok(draft()),
        })
        .unwrap();
    engine.poll();
    let job = engine
        .prepare_job()
        .expect("preparation starts with the plan");
    assert_eq!(job.total, 3);
    engine.handle(Command::CancelPrepare).unwrap();
    assert!(engine.prepare_job().unwrap().is_cancelling());
}

#[test]
fn prepared_question_bank_fills_quizzes_without_model_calls() {
    let (mut engine, _dir) = engine_with_plan();
    let topic = first_topic(&engine);
    let bank: Vec<QuestionDraft> = (0..10)
        .map(|n| QuestionDraft {
            prompt: format!("Questão {n} sobre crase?"),
            options: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            correct: n % 4,
            explanation: "Porque.".into(),
        })
        .collect();
    engine
        .events_tx
        .send(Event::Prepare(prepare::PrepareEvent::QuizBank {
            topic,
            json: serde_json::to_string(&bank).unwrap(),
        }))
        .unwrap();
    engine.poll();

    engine.handle(Command::StartQuiz(topic)).unwrap();
    let quiz = engine.quiz().unwrap();
    assert!(matches!(quiz.step, QuizStep::Question { index: 0, .. }));
    assert_eq!(quiz.questions().len(), 5);
    assert!(quiz.questions().iter().all(|q| !q.from_board));
}

#[test]
fn sentences_of_an_older_reply_are_ignored() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartLesson(id)).unwrap();
    let lesson_id = engine.lesson().unwrap().id;
    // A reply the student talked over, or the lesson id itself, is not the
    // reply being received.
    engine
        .events_tx
        .send(Event::Lesson(LessonEvent::Sentence {
            session: lesson_id,
            text: "Velho.".into(),
        }))
        .unwrap();
    engine.poll();
    assert!(engine.lesson().unwrap().turns.is_empty());
}

/// Polls until background work makes `done` true (or ten seconds pass).
fn poll_until(engine: &mut Engine, done: impl Fn(&Engine) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !done(engine) && std::time::Instant::now() < deadline {
        engine.poll();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(done(engine), "background work did not finish");
}

#[test]
fn generated_handout_opens_in_the_reader_with_progress_and_highlights() {
    let (mut engine, dir) = engine_with_plan();
    let document = kg_handout::Document {
        title: "Plano".into(),
        subtitle: "Crase".into(),
        generated: "Hoje".into(),
        lang: "pt".into(),
        labels: HandoutLabels::default(),
        topics: vec![kg_handout::TopicHandout {
            topic: "Crase".into(),
            subject: "Português".into(),
            introduction: "A crase marca a fusão da preposição com o artigo.".into(),
            sections: vec![kg_handout::Section {
                heading: "Quando usar".into(),
                paragraphs: vec!["Antes de palavra feminina que aceita artigo.".into()],
                ..Default::default()
            }],
            ..Default::default()
        }],
        files: Vec::new(),
        marks: Vec::new(),
    };
    let saved =
        crate::handouts::saved_handout(document, &dir.path().join("crase.pdf"), None).unwrap();
    engine.on_handout_finished(Ok(saved)).unwrap();
    assert_eq!(engine.readings().len(), 1);
    assert!(
        engine
            .take_notices()
            .iter()
            .any(|n| matches!(n, Notice::HandoutSaved(_)))
    );

    poll_until(&mut engine, |e| {
        e.reader().is_some_and(|r| r.pages.is_some())
    });
    let pages = engine.reader().unwrap().pages.clone().unwrap();
    assert_eq!(engine.reader().unwrap().reading.page_count, pages.len());
    let page = (0..pages.len())
        .find(|p| pages.text_of(*p, 0..pages.text(*p).len()).contains("fusão"))
        .unwrap();
    // The contents point at the body, not at the contents page.
    let contents = &engine.reader().unwrap().contents;
    let titles: Vec<(&str, u8)> = contents
        .iter()
        .map(|c| (c.title.as_str(), c.level))
        .collect();
    assert_eq!(titles, [("Crase", 0), ("Quando usar", 1)]);
    assert_eq!(contents[0].page, page);

    engine.handle(Command::ReaderGoTo(page)).unwrap();
    engine
        .handle(Command::RenderPage { page, scale: 50 })
        .unwrap();
    poll_until(&mut engine, |e| {
        e.reader().unwrap().page_image(page, 50).is_some()
    });

    let boxes = pages.text(page).len();
    engine
        .handle(Command::AddHighlight {
            page,
            first: 0,
            last: boxes - 1,
            color: HighlightColor::Green,
        })
        .unwrap();
    let highlight = engine.reader().unwrap().highlights[0].clone();
    assert!(highlight.text.contains("fusão"));
    engine
        .handle(Command::UpdateHighlight {
            id: highlight.id,
            color: HighlightColor::Pink,
            note: " revisar ".into(),
        })
        .unwrap();
    assert_eq!(engine.reader().unwrap().highlights[0].note, "revisar");

    let marked = dir.path().join("crase-marcada.pdf");
    engine
        .handle(Command::ExportReading(marked.clone()))
        .unwrap();
    poll_until(&mut engine, |_| marked.exists());

    // Progress is kept after closing and comes back on reopening.
    engine.handle(Command::CloseReader).unwrap();
    let reading = engine.readings()[0].clone();
    assert!(reading.pages_seen.contains(&page));
    assert_eq!(reading.last_page, page);
    engine.handle(Command::OpenReading(reading.id)).unwrap();
    assert_eq!(engine.reader().unwrap().highlights.len(), 1);

    engine.handle(Command::DeleteReading(reading.id)).unwrap();
    assert!(engine.reader().is_none() && engine.readings().is_empty());
}

#[test]
fn wrong_answers_go_to_the_notebook_and_open_the_next_quiz() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    let draft = |prompt: &str| QuestionDraft {
        prompt: prompt.into(),
        options: vec!["a".into(), "b".into(), "c".into()],
        correct: 0,
        explanation: String::new(),
    };
    let ready = |engine: &mut Engine, prompts: &[&str]| {
        let session = engine.quiz().unwrap().id;
        engine
            .events_tx
            .send(Event::Practice(PracticeEvent::QuizReady {
                session,
                result: Ok(prompts.iter().map(|p| draft(p)).collect()),
            }))
            .unwrap();
        engine.poll();
    };
    let right = |engine: &Engine| engine.quiz().unwrap().current().unwrap().correct;

    engine.handle(Command::StartQuiz(id)).unwrap();
    ready(&mut engine, &["Primeira?", "Segunda?"]);
    let missed = engine.quiz().unwrap().current().unwrap().prompt.clone();
    let wrong = (right(&engine) + 1) % 3;
    engine.handle(Command::AnswerQuiz(wrong)).unwrap();
    assert_eq!(engine.mistakes().len(), 1);
    assert_eq!(engine.mistakes()[0].question.prompt, missed);
    // The tutor and the reviews hear about it.
    assert_eq!(engine.topic_context(id).unwrap().0.mistakes.len(), 1);

    // The next quiz opens with it, and a right answer clears it.
    engine.handle(Command::StartQuiz(id)).unwrap();
    ready(&mut engine, &["Outra?"]);
    let quiz = engine.quiz().unwrap();
    assert_eq!(quiz.current().unwrap().prompt, missed);
    assert!(quiz.is_retry(0) && !quiz.is_retry(1));
    engine.handle(Command::AnswerQuiz(right(&engine))).unwrap();
    assert!(engine.mistakes().is_empty());
}

#[test]
fn going_over_mistakes_is_not_a_review() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartQuiz(id)).unwrap();
    let session = engine.quiz().unwrap().id;
    engine
        .events_tx
        .send(Event::Practice(PracticeEvent::QuizReady {
            session,
            result: Ok(vec![QuestionDraft {
                prompt: "Qual?".into(),
                options: vec!["a".into(), "b".into()],
                correct: 0,
                explanation: String::new(),
            }]),
        }))
        .unwrap();
    engine.poll();
    let wrong = (engine.quiz().unwrap().current().unwrap().correct + 1) % 2;
    engine.handle(Command::AnswerQuiz(wrong)).unwrap();
    engine.handle(Command::NextQuestion).unwrap();
    let attempts = engine.quiz_stats().attempts;
    let due = topic(&engine, id).review.due;

    engine.handle(Command::RetryMistakes(id)).unwrap();
    let quiz = engine.quiz().unwrap();
    assert!(quiz.mistakes_only);
    assert_eq!(quiz.questions().len(), 1);
    let right = quiz.current().unwrap().correct;
    engine.handle(Command::AnswerQuiz(right)).unwrap();
    engine.handle(Command::NextQuestion).unwrap();
    assert!(engine.mistakes().is_empty());
    assert_eq!(engine.quiz_stats().attempts, attempts);
    assert_eq!(topic(&engine, id).review.due, due);
}

#[test]
fn an_unfinished_lesson_is_kept_and_picked_up_where_it_stopped() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartLesson(id)).unwrap();
    engine.simulate_tutor_text("Oi! Vamos lá.\nROTEIRO: O que é | Quando usar | Exceções");
    engine
        .handle(Command::SendMessage("Crase é a fusão de dois as.".into()))
        .unwrap();
    engine.simulate_tutor_text("Isso!\nETAPA: 2");
    engine.handle(Command::EndLesson).unwrap();
    // Not taken, but kept, and it counts as study today.
    assert!(engine.has_lesson_draft(id));
    assert!(!topic(&engine, id).lesson_taken());
    assert!(engine.studied_today());

    engine.handle(Command::ResumeLesson(id)).unwrap();
    let lesson = engine.lesson().unwrap();
    assert_eq!((lesson.step, lesson.outline.len()), (1, 3));
    let texts: Vec<&str> = lesson.turns.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(
        texts,
        ["Oi! Vamos lá.", "Crase é a fusão de dois as.", "Isso!"]
    );

    engine.simulate_tutor_text("Resumo: crase é fusão. Parabéns!\nFIM");
    assert!(!engine.has_lesson_draft(id));
    assert!(topic(&engine, id).lesson_taken());
    engine.handle(Command::EndLesson).unwrap();

    engine.handle(Command::OpenRecap(id)).unwrap();
    let (topic_id, recap) = engine.recap().unwrap();
    let recap = recap.unwrap();
    assert_eq!(topic_id, id);
    assert_eq!(recap.outline, ["O que é", "Quando usar", "Exceções"]);
    assert_eq!(recap.summary, "Resumo: crase é fusão. Parabéns!");
    // Starting the lesson again closes the recap.
    engine.handle(Command::StartLesson(id)).unwrap();
    assert!(engine.recap().is_none());
}

#[test]
fn starting_over_drops_the_unfinished_lesson() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartLesson(id)).unwrap();
    engine.simulate_tutor_text("Oi!");
    engine.handle(Command::SendMessage("Oi.".into())).unwrap();
    engine.handle(Command::EndLesson).unwrap();
    assert!(engine.has_lesson_draft(id));
    engine.handle(Command::StartLesson(id)).unwrap();
    assert!(!engine.has_lesson_draft(id));
    assert!(engine.lesson().unwrap().turns.is_empty());
}

#[test]
fn a_review_asks_three_questions_and_suggests_the_average_grade() {
    let (mut engine, _dir) = engine_with_plan();
    let id = first_topic(&engine);
    engine.handle(Command::StartReview(id)).unwrap();
    let session = engine.review().unwrap().id;
    let send = |engine: &mut Engine, event: PracticeEvent| {
        engine.events_tx.send(Event::Practice(event)).unwrap();
        engine.poll();
    };
    for (round, score) in [(0, 5u8), (1, 2), (2, 4)] {
        assert_eq!(engine.review().unwrap().round, round);
        send(
            &mut engine,
            PracticeEvent::ReviewQuestion {
                session,
                result: Ok(format!("Pergunta {round}?")),
            },
        );
        if round == 1 {
            // A spoken answer goes after what was typed.
            send(
                &mut engine,
                PracticeEvent::ReviewHeard {
                    session,
                    typed: "Escrevi isto.".into(),
                    result: Ok("e falei isto".into()),
                },
            );
            match &engine.review().unwrap().step {
                ReviewStep::Grading { answer, .. } => {
                    assert_eq!(answer, "Escrevi isto. e falei isto");
                }
                step => panic!("unexpected {step:?}"),
            }
        } else {
            engine
                .handle(Command::SubmitReviewAnswer("Resposta.".into()))
                .unwrap();
        }
        send(
            &mut engine,
            PracticeEvent::ReviewGraded {
                session,
                result: Ok(ReviewFeedback {
                    score,
                    feedback: "Ok.".into(),
                }),
            },
        );
        engine.handle(Command::NextReviewQuestion).unwrap();
    }
    let review = engine.review().unwrap();
    assert!(review.is_last_round());
    match &review.step {
        // (5 + 2 + 4) / 3 rounds to 4.
        ReviewStep::Graded { suggested, .. } => assert_eq!(*suggested, ReviewGrade::Good),
        step => panic!("unexpected {step:?}"),
    }
    engine
        .handle(Command::GradeReview(ReviewGrade::Good))
        .unwrap();
    assert!(engine.review().is_none());
}
