//! Renders every screen to PNG for visual review. Needs a GPU, so it is
//! ignored by default:
//!
//! KG_SNAPSHOT_DIR=<dir> cargo test -p kg-ui snapshots -- --ignored

use chrono::Days;
use egui_kittest::Harness;
use kg_engine::{AiConfig, AppPaths, Command, Engine};
use kg_study::{
    Difficulty, DocumentKind, NewBoardQuestion, NewPlan, NewSubject, NewTopic, Relevance,
    ReviewGrade, Store, TopicStatus,
};

use crate::app::{KnowGiverApp, Screen};

/// Demo content in the screenshots' language: Portuguese, or English for
/// the README's English screenshots.
fn tr(english: bool, pt: &'static str, en: &'static str) -> &'static str {
    if english { en } else { pt }
}

/// The demo plan's notice: it has an essay, so written answers are offered.
const NOTICE: &str = "EDITAL
12. DA PROVA DISCURSIVA
12.1 A prova discursiva consistirá na redação de texto dissertativo, de 20 a 30 linhas, valendo 20 pontos.
12.2 Serão avaliados a apresentação e a estrutura textual (5 pontos) e o desenvolvimento do tema (15 pontos).";

fn seed(paths: &AppPaths, english: bool) {
    let tr = |pt, en| tr(english, pt, en);
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let mut store = Store::open(&paths.database()).unwrap();
    let topic = |title: &str, difficulty, week| NewTopic {
        title: title.into(),
        difficulty,
        week,
    };
    let plan = NewPlan::new(
        tr(
            "Plano TJCE – Técnico Judiciário",
            "State Court – Court Clerk",
        ),
        tr(
            "Tribunal de Justiça do Ceará – Técnico Judiciário – Área Judiciária",
            "Ceará State Court – Court Clerk – Judicial Area",
        ),
        10,
        vec![
            NewSubject {
                name: tr("Língua Portuguesa", "Language and Writing").into(),
                topics: vec![
                    topic(
                        tr(
                            "Compreensão e interpretação de textos",
                            "Reading comprehension",
                        ),
                        Difficulty::Medium,
                        1,
                    ),
                    topic(
                        tr(
                            "Emprego do sinal indicativo de crase",
                            "Subject–verb agreement",
                        ),
                        Difficulty::Hard,
                        1,
                    ),
                    topic(
                        tr("Regência verbal e nominal", "Verbs and prepositions"),
                        Difficulty::Hard,
                        2,
                    ),
                    topic(tr("Pontuação", "Punctuation"), Difficulty::Easy, 3),
                ],
            },
            NewSubject {
                name: tr("Noções de Direito Constitucional", "Constitutional Law").into(),
                topics: vec![
                    topic(
                        tr("Princípios fundamentais", "Fundamental principles"),
                        Difficulty::Easy,
                        1,
                    ),
                    topic(
                        tr("Direitos e garantias fundamentais", "Fundamental rights"),
                        Difficulty::Hard,
                        2,
                    ),
                    topic(
                        tr("Poder Judiciário", "The Judiciary"),
                        Difficulty::Medium,
                        3,
                    ),
                ],
            },
        ],
    )
    .unwrap();
    let today = Engine::today();
    let start = today.checked_sub_days(Days::new(3)).unwrap();
    let mut plan = store
        .insert_plan(
            &plan,
            tr("edital-tjce.pdf", "exam-notice.pdf"),
            NOTICE,
            start,
        )
        .unwrap();
    let topics = &mut plan.subjects[0].topics;
    topics[0].complete_lesson(start);
    topics[0].apply_review(ReviewGrade::Good, start);
    topics[1].complete_lesson(start);
    topics[2].status = TopicStatus::Studying;
    topics[3].status = TopicStatus::Done;
    for t in topics.iter() {
        store.update_topic(t).unwrap();
    }
    let t = &mut plan.subjects[1].topics[0];
    t.complete_lesson(today);
    store.update_topic(t).unwrap();

    let crase = plan.subjects[0].topics[1].id;
    // A question missed in a quiz, and a lesson left halfway.
    store
        .record_mistake(
            crase,
            &kg_engine::QuizQuestion {
                prompt: tr(
                    "Há crase em \"Vou a pé\"?",
                    "Which is right: \"The list of names is\" or \"are\"?",
                )
                .into(),
                options: vec![tr("Sim", "is").into(), tr("Não", "are").into()],
                correct: if english { 0 } else { 1 },
                explanation: tr(
                    "Não há crase antes de palavra masculina.",
                    "The verb agrees with \"list\", the head noun.",
                )
                .into(),
                from_board: false,
            },
            if english { 1 } else { 0 },
            today,
        )
        .unwrap();
    let halfway = plan.subjects[0].topics[2].id;
    store
        .save_lesson_draft(
            halfway,
            r#"{"outline":["Um","Dois"],"step":1,"board":[],"turns":[{"speaker":"Tutor","text":"Oi"},{"speaker":"Student","text":"Oi"}]}"#,
            today,
        )
        .unwrap();
    let question = |n: u32, prompt: &str, topic, relevance, reason: &str| NewBoardQuestion {
        number: Some(n),
        prompt: prompt.into(),
        options: vec![tr("Certo", "True").into(), tr("Errado", "False").into()],
        correct: Some(0),
        topic_id: topic,
        relevance,
        reason: reason.into(),
    };
    store
        .insert_board_document(
            plan.id,
            DocumentKind::Exam,
            tr("prova-trf6-2024.pdf", "past-exam-2024.pdf"),
            "texto",
            &[
                question(12, tr("A supressão do acento indicativo de crase em “àquilo” manteria a correção gramatical do texto.", "In “The list of candidates were published”, the verb agrees correctly with its subject."), Some(crase), Relevance::Relevant, ""),
                question(13, tr("No trecho “O direito se constitui em esfera especial”, o pronome poderia ser deslocado para depois do verbo.", "In “Neither the judge nor the clerks was present”, the verb should agree with “clerks”."), Some(crase), Relevance::Partial, ""),
                question(17, tr("Considerando a Lei n.º 8.112/1990, é proibido ao servidor atuar como procurador junto a repartições públicas.", "Under the civil service statute, a public servant may not act as an agent before public offices."), None, Relevance::OutOfScope, tr("Lei 8.112/1990 não consta no edital", "The civil service statute is not in the exam notice")),
            ],
            &[],
            today,
        )
        .unwrap();
    drop(store);
    finish_setup(paths, english);
}

/// Saves the first-run choices, so screenshots open on the screens.
fn finish_setup(paths: &AppPaths, english: bool) {
    let mut engine = Engine::new(paths.clone(), AiConfig::default()).unwrap();
    let mut settings = engine.settings().clone();
    settings.student_name = tr(english, "Ana", "Anna").into();
    settings.teacher = Some(settings.teacher());
    settings.locale = if english { "en" } else { "pt-BR" }.into();
    engine
        .handle(Command::CompleteSetup(Box::new(settings)))
        .unwrap();
    engine.handle(Command::FinishTour).unwrap();
}

/// The first-run setup, its AI and terms steps, and the tour that follows.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn setup_snapshots() {
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    for locale in ["pt-BR", "en"] {
        let data = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(data.path());
        crate::i18n::set(locale);
        let mut harness = Harness::builder()
            .with_size([1180.0, 900.0])
            .build_eframe(|cc| KnowGiverApp::with_paths(cc, paths.clone(), AiConfig::default()));
        crate::i18n::set(locale);
        render(
            &mut harness,
            out.as_deref(),
            &format!("{locale}-setup-language"),
        );

        let setup = &mut harness.state_mut().ui.setup;
        setup.language_chosen = true;
        if let Some(draft) = &mut setup.draft {
            draft.student_name = "Ana".into();
            draft.teacher = Some(kg_engine::Teacher::ALL[1]);
        }
        setup.step = 2;
        render(
            &mut harness,
            out.as_deref(),
            &format!("{locale}-setup-teacher"),
        );

        let setup = &mut harness.state_mut().ui.setup;
        let zai = kg_engine::ai_provider("zai").unwrap();
        setup.provider = Some(zai);
        setup.model = zai.default_model().into();
        setup.base_url = zai.base_url.into();
        setup.api_key = "sk-demo-key".into();
        setup.step = 3;
        render(&mut harness, out.as_deref(), &format!("{locale}-setup-ai"));

        let setup = &mut harness.state_mut().ui.setup;
        setup.step = 4;
        setup.agreed = true;
        setup.reading = Some(crate::legal::LegalDoc::Terms);
        render(
            &mut harness,
            out.as_deref(),
            &format!("{locale}-setup-terms"),
        );

        // Finishing opens the tour.
        let settings = {
            let setup = &harness.state().ui.setup;
            let mut s = setup.draft.clone().unwrap();
            s.ai = zai.config(zai.base_url, "sk-demo-key", zai.default_model());
            s.ai_provider = zai.id.into();
            s.locale = locale.into();
            s
        };
        harness
            .state_mut()
            .dispatch(Command::CompleteSetup(Box::new(settings)));
        harness.state_mut().ui.tour_step = 5;
        render(&mut harness, out.as_deref(), &format!("{locale}-tour"));
    }
}

/// The provider list as it opens, with logos and groups.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn provider_list_snapshot() {
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    crate::i18n::set("pt-BR");
    let mut harness = Harness::builder().with_size([460.0, 860.0]).build_ui(|ui| {
        // Fonts load between frames: set them up first, draw from the next one.
        let ready = egui::Id::new("theme_ready");
        if !ui
            .ctx()
            .data(|d| d.get_temp::<bool>(ready))
            .unwrap_or(false)
        {
            crate::theme::apply(ui.ctx());
            egui_extras::install_image_loaders(ui.ctx());
            ui.ctx().data_mut(|d| d.insert_temp(ready, true));
            return;
        }
        egui::Frame::new()
            .fill(crate::theme::palette(ui.ctx()).surface)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                crate::screens::setup::provider_list(ui, Some("anthropic"));
            });
    });
    harness.run_steps(4);
    std::thread::sleep(std::time::Duration::from_millis(150));
    harness.run_steps(4);
    let image = harness.render().unwrap();
    if let Some(dir) = out.as_deref() {
        image.save(dir.join("providers.png")).unwrap();
    }
}

fn render(harness: &mut Harness<'_, KnowGiverApp>, dir: Option<&std::path::Path>, name: &str) {
    // A few frames more than needed, so engravings finish decoding.
    harness.run_steps(4);
    std::thread::sleep(std::time::Duration::from_millis(150));
    harness.run_steps(4);
    let image = harness.render().unwrap();
    if let Some(dir) = dir {
        image.save(dir.join(format!("{name}.png"))).unwrap();
    }
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn snapshots() {
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);

    for locale in ["pt-BR", "en"] {
        let english = locale == "en";
        let data = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(data.path());
        seed(&paths, english);
        crate::i18n::set(locale);
        let mut harness = Harness::builder()
            .with_size([1180.0, 760.0])
            .build_eframe(|cc| KnowGiverApp::with_paths(cc, paths.clone(), AiConfig::default()));
        // The app applies the saved language on start; override it here.
        crate::i18n::set(locale);
        for (screen, name) in [
            (Screen::Today, "today"),
            (Screen::Welcome, "welcome"),
            (Screen::Edital, "edital"),
            (Screen::Plan, "plan"),
            (Screen::Board, "board"),
            (Screen::Review, "review"),
            (Screen::Progress, "progress"),
            (Screen::Settings, "settings"),
            (Screen::Quiz, "quiz"),
            (Screen::Lesson, "lesson"),
        ] {
            harness.state_mut().ui.screen = screen;
            render(&mut harness, out.as_deref(), &format!("{locale}-{name}"));
        }
        // Adding a past exam asks about its answer key.
        harness.state_mut().ui.screen = Screen::Board;
        harness
            .ctx
            .data_mut(|d| d.insert_temp(egui::Id::new("board_exam_key_question"), true));
        render(&mut harness, out.as_deref(), &format!("{locale}-board-key"));
        harness
            .ctx
            .data_mut(|d| d.remove::<bool>(egui::Id::new("board_exam_key_question")));

        // A lesson in progress.
        let topic = harness
            .state()
            .engine
            .as_ref()
            .ok()
            .and_then(|e| e.active_plan())
            .map(|p| p.subjects[0].topics[1].id)
            .unwrap();
        harness.state_mut().dispatch(Command::StartLesson(topic));
        if let Ok(engine) = harness.state_mut().engine.as_mut() {
            engine.simulate_tutor_reply(
                tr(
                    english,
                    "Olá! Hoje vamos ver quando usar a crase. Repare: ela é a fusão da preposição a com o artigo a. Você já sabe quando ela é obrigatória?",
                    "Hi! Today we look at subject–verb agreement. The verb follows the head of the subject, not the nearest noun. Which is right: “The list of names is” or “are”?",
                ),
                &[
                    tr(english, "Crase = preposição a + artigo a", "The verb agrees with the head noun"),
                    tr(english, "Teste: troque por palavra masculina", "Test: drop the “of …” phrase"),
                ],
            );
            // The lesson plan, on its second step.
            engine.simulate_tutor_text(tr(
                english,
                "ROTEIRO: O que é crase | Casos obrigatórios | Casos proibidos | Pegadinhas de prova
ETAPA: 2",
                "ROTEIRO: Find the head noun | Tricky subjects | Collective nouns | Exam traps
ETAPA: 2",
            ));
        }
        harness.state_mut().ui.screen = Screen::Lesson;
        render(
            &mut harness,
            out.as_deref(),
            &format!("{locale}-lesson-active"),
        );
        harness.state_mut().ui.lesson_panel = true;
        render(
            &mut harness,
            out.as_deref(),
            &format!("{locale}-lesson-panel"),
        );
        harness.state_mut().ui.lesson_panel = false;
        if let Ok(engine) = harness.state_mut().engine.as_mut() {
            engine.simulate_tutor_text(tr(
                english,
                "Parabéns, você fechou o tópico!
FIM",
                "Well done, you closed the topic!
FIM",
            ));
        }
        render(
            &mut harness,
            out.as_deref(),
            &format!("{locale}-lesson-finished"),
        );
        harness.state_mut().dispatch(Command::EndLesson);
        harness.state_mut().dispatch(Command::OpenRecap(topic));
        render(
            &mut harness,
            out.as_deref(),
            &format!("{locale}-lesson-recap"),
        );
        harness.state_mut().dispatch(Command::CloseRecap);
        // The "lesson recorded" message would sit on every later screenshot.
        harness.run();
        harness.state_mut().clear_toasts();

        // An edital with several roles, waiting for the choice.
        let roles = tr(
            english,
            "2.1.1 CARGO 1: AUDITOR FEDERAL DE FINANÇAS E CONTROLE (AFFC) – ESPECIALIDADE: AUDITORIA\n\n\
             2.1.2 CARGO 2: AUDITOR FEDERAL DE FINANÇAS E CONTROLE (AFFC) – ESPECIALIDADE: TECNOLOGIA DA\n\
             INFORMAÇÃO – CIÊNCIA DE DADOS\n\n\
             2.1.3 CARGO 3: AUDITOR FEDERAL DE FINANÇAS E CONTROLE (AFFC) – ESPECIALIDADE: TECNOLOGIA DA\n\
             INFORMAÇÃO – INFRAESTRUTURA DE TI\n",
            "2.1.1 CARGO 1: FEDERAL FINANCE AND CONTROL AUDITOR – SPECIALTY: AUDIT\n\n\
             2.1.2 CARGO 2: FEDERAL FINANCE AND CONTROL AUDITOR – SPECIALTY: INFORMATION\n\
             TECHNOLOGY – DATA SCIENCE\n\n\
             2.1.3 CARGO 3: FEDERAL FINANCE AND CONTROL AUDITOR – SPECIALTY: INFORMATION\n\
             TECHNOLOGY – IT INFRASTRUCTURE\n",
        );
        if let Ok(engine) = harness.state_mut().engine.as_mut() {
            engine.simulate_import(kg_engine::Edital {
                file_name: tr(english, "edital-cgu.pdf", "exam-notice-cgu.pdf").into(),
                pages: 69,
                text: kg_engine::EditalText::from_raw(roles),
            });
        }
        harness.state_mut().ui.prefs.role = tr(
            english,
            "Auditor Federal de Finanças e Controle (AFFC) – Especialidade: Tecnologia da Informação – Ciência de Dados",
            "Federal Finance and Control Auditor – Specialty: Information Technology – Data Science",
        )
        .into();
        harness.state_mut().ui.screen = Screen::Edital;
        render(
            &mut harness,
            out.as_deref(),
            &format!("{locale}-edital-roles"),
        );

        reader_snapshots(&mut harness, out.as_deref(), locale, english, data.path());
    }
    crate::i18n::set(crate::i18n::DEFAULT_LOCALE);
}

/// A generated handout open in the reader, with a highlight and a note, and
/// the handout library.
fn reader_snapshots(
    harness: &mut Harness<'_, KnowGiverApp>,
    out: Option<&std::path::Path>,
    locale: &str,
    english: bool,
    dir: &std::path::Path,
) {
    use kg_engine::{
        HandoutDocument, HandoutExercise, HandoutLabels, HandoutSection, TopicHandout,
    };
    let tr = |pt, en| tr(english, pt, en);
    let labels = HandoutLabels {
        handout: tr("Apostila", "Study handout").into(),
        contents: tr("Sumário", "Contents").into(),
        key_points: tr("Pontos-chave", "Key points").into(),
        examples: tr("Exemplos", "Examples").into(),
        pitfalls: tr("Pegadinhas de prova", "Exam pitfalls").into(),
        summary: tr("Resumo", "Summary").into(),
        exercises: tr("Exercícios", "Exercises").into(),
        answers: tr("Gabarito comentado", "Answer key").into(),
        picture_source: tr("Imagem: Wikipédia", "Picture: Wikipedia").into(),
        answer_grid: tr("Respostas rápidas", "Quick answers").into(),
    };
    let document = HandoutDocument {
        title: tr("Plano TJCE – Técnico Judiciário", "State Court – Court Clerk").into(),
        subtitle: tr("Emprego do sinal indicativo de crase", "Subject–verb agreement").into(),
        generated: tr("Gerada em 28/10/2026", "Generated on Oct 28, 2026").into(),
        lang: tr("pt", "en").into(),
        labels,
        topics: vec![TopicHandout {
            topic: tr("Emprego do sinal indicativo de crase", "Subject–verb agreement").into(),
            subject: tr("Língua Portuguesa", "Language and Writing").into(),
            introduction: tr(
                "A crase é a fusão da preposição a com o artigo feminino a. Ela cai em quase toda prova de português, quase sempre em frases curtas que testam se você reconhece os dois elementos.",
                "A verb agrees with the head of its subject, not with the noun closest to it. Exams test this with long subjects whose last noun has the other number.",
            )
            .into(),
            sections: vec![
                HandoutSection {
                    heading: tr("Quando usar", "Find the head noun").into(),
                    paragraphs: vec![tr(
                        "Use a crase quando o termo regente exige a preposição a e a palavra seguinte aceita o artigo a. O teste mais rápido é trocar a palavra feminina por uma masculina: se aparecer ao, há crase.",
                        "Strip the phrases that start with of, with or in. What is left is the head noun, and the verb follows it: the list of names is long, the boxes on the shelf are heavy.",
                    )
                    .into()],
                    key_points: vec![tr("Troque por masculino: ao indica crase.", "Drop the of-phrase to find the head.").into()],
                    ..Default::default()
                },
                HandoutSection {
                    heading: tr("Casos proibidos", "Tricky subjects").into(),
                    paragraphs: vec![tr(
                        "Não há crase antes de palavras masculinas, de verbos, de pronomes pessoais e de expressões com palavras repetidas, como cara a cara.",
                        "Each and every take a singular verb. Neither ... nor and either ... or agree with the nearer part of the subject.",
                    )
                    .into()],
                    ..Default::default()
                },
            ],
            pitfalls: vec![tr("A partir de nunca tem crase.", "A number of takes a plural verb; the number of takes a singular one.").into()],
            exercises: vec![HandoutExercise {
                prompt: tr("Assinale a frase correta.", "Choose the correct sentence.").into(),
                options: vec![
                    tr("Fui à pé.", "The list of names are long.").into(),
                    tr("Fui à praia.", "The list of names is long.").into(),
                ],
                correct: 1,
                explanation: tr("Praia é feminina e aceita o artigo.", "The head noun is list, singular.").into(),
            }],
            ..Default::default()
        }],
        files: Vec::new(),
        marks: Vec::new(),
    };
    if let Ok(engine) = harness.state_mut().engine.as_mut() {
        engine.simulate_handout(document, dir).unwrap();
    }
    // Pages are laid out on a worker thread.
    for _ in 0..100 {
        harness.run_steps(2);
        let ready = harness
            .state()
            .engine
            .as_ref()
            .ok()
            .and_then(|e| e.reader())
            .is_some_and(|r| r.pages.is_some());
        if ready {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    // Highlight the section's first sentence, with a note.
    let target = harness
        .state()
        .engine
        .as_ref()
        .ok()
        .and_then(|e| e.reader())
        .and_then(|r| r.pages.clone())
        .and_then(|pages| {
            let needle = tr("Use a crase", "Strip the");
            (0..pages.len()).find_map(|page| {
                let boxes = pages.text(page);
                let text: Vec<&str> = boxes.iter().map(|b| b.text.as_str()).collect();
                let joined = text.concat();
                let at = joined.find(needle)?;
                let mut chars = 0;
                let first = boxes.iter().position(|b| {
                    chars += b.text.len();
                    chars > at
                })?;
                Some((page, first, (first + 120).min(boxes.len() - 1)))
            })
        });
    if let Some((page, first, last)) = target {
        harness.state_mut().dispatch(Command::AddHighlight {
            page,
            first,
            last,
            color: kg_engine::HighlightColor::Yellow,
        });
        let id = harness
            .state()
            .engine
            .as_ref()
            .ok()
            .and_then(|e| e.reader())
            .and_then(|r| r.highlights.first().map(|h| h.id));
        if let Some(id) = id {
            harness.state_mut().dispatch(Command::UpdateHighlight {
                id,
                color: kg_engine::HighlightColor::Yellow,
                note: tr("Cai em toda prova.", "Shows up in every exam.").into(),
            });
        }
        harness.state_mut().ui.reader.jump_to(page);
    }
    harness.state_mut().ui.screen = Screen::Reader;
    // Let the pages render and the textures load.
    for _ in 0..6 {
        harness.run_steps(4);
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
    render(harness, out, &format!("{locale}-reader"));
    harness.state_mut().dispatch(Command::CloseReader);
    render(harness, out, &format!("{locale}-reader-library"));
}

/// Every teacher in each lesson phase: one row per teacher.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn avatar_states() {
    use kg_engine::{LessonPhase, Teacher};
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    let phases = [
        (LessonPhase::Idle, 0.0, 0.0),
        (LessonPhase::Listening, 0.0, 0.7),
        (LessonPhase::Thinking, 0.0, 0.0),
        (LessonPhase::Speaking, 0.3, 0.0),
        (LessonPhase::Speaking, 0.9, 0.0),
    ];
    let mut harness = Harness::builder()
        .with_size([1120.0, 866.0])
        .build_ui(move |ui| {
            crate::theme::apply(ui.ctx());
            egui_extras::install_image_loaders(ui.ctx());
            for teacher in Teacher::ALL {
                ui.horizontal(|ui| {
                    for (phase, speech, mic) in phases {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(210.0, 205.0), egui::Sense::hover());
                        crate::teacher::paint(
                            ui,
                            rect,
                            crate::teacher::Mood { phase, speech, mic },
                            teacher,
                        );
                    }
                });
            }
        });
    // The mouth eases toward the voice level; give it time, and SVGs time to load.
    for _ in 0..4 {
        harness.run_steps(10);
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    let image = harness.render().unwrap();
    if let Some(dir) = out {
        image.save(dir.join("avatar-states.png")).unwrap();
    }
}

/// New visual primitives in light and dark themes, without external models or AI.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn roadmap_components() {
    use crate::{icons, illustrations, teacher, whiteboard};
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    for (name, theme) in [("light", egui::Theme::Light), ("dark", egui::Theme::Dark)] {
        let mut fonts_ready = false;
        let mut harness = Harness::builder()
            .with_size([1040.0, 1180.0])
            .build_ui(move |ui| {
                crate::theme::apply(ui.ctx());
                ui.ctx().set_theme(theme);
                // Fonts set in a frame are used from the next one.
                if !std::mem::replace(&mut fonts_ready, true) {
                    return;
                }
                egui_extras::install_image_loaders(ui.ctx());
                let palette = crate::theme::palette(ui.ctx());
                ui.horizontal(|ui| {
                    ui.add(icons::logo(48.0));
                    ui.heading("Know Giver");
                });
                ui.horizontal(|ui| {
                    for kind in [
                        icons::Icon::Edital,
                        icons::Icon::Plan,
                        icons::Icon::Quiz,
                        icons::Icon::Progress,
                    ] {
                        illustrations::empty(ui, kind);
                    }
                });
                ui.horizontal(|ui| {
                    for who in kg_engine::Teacher::ALL {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(190.0, 190.0), egui::Sense::hover());
                        teacher::paint(
                            ui,
                            rect,
                            teacher::Mood {
                                phase: kg_engine::LessonPhase::Idle,
                                speech: 0.0,
                                mic: 0.0,
                            },
                            who,
                        );
                    }
                });
                for pair in [
                    [
                        "FORMULA: interest = principal × rate × time",
                        "DIAGRAMA: Exam notice -> Study plan -> Review",
                    ],
                    [
                        "TABELA: Law | Term | Passed by ; Multi-year plan | 4 years | Congress ; Budget guidelines | 1 year | Congress ; Annual budget | 1 year | Congress",
                        "LINHA DO TEMPO: 1964: Budget law | 1988: Constitution | 2000: Fiscal responsibility | 2021: New procurement law",
                    ],
                    [
                        "MAPA: Public budget: Multi-year plan | Guidelines | Annual budget | Extra credits",
                        "DESTAQUE: The annual budget may only deal with revenue and spending.",
                    ],
                ] {
                    ui.horizontal(|ui| {
                        for text in pair {
                            let (area, _) = ui
                                .allocate_exact_size(egui::vec2(485.0, 200.0), egui::Sense::hover());
                            whiteboard::paint(
                                ui,
                                &ui.painter_at(area),
                                area.shrink(14.0),
                                &whiteboard::parse(text),
                                palette.text,
                                None,
                            );
                        }
                    });
                }
            });
        harness.run_steps(8);
        let image = harness.render().unwrap();
        if let Some(dir) = &out {
            image.save(dir.join(format!("roadmap-{name}.png"))).unwrap();
        }
    }
}

/// Voice choices and the optional remote-service notice in both supported languages.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn voice_settings_states() {
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    for locale in ["pt-BR", "en"] {
        crate::i18n::set(locale);
        for backend in [kg_engine::TtsBackend::Local, kg_engine::TtsBackend::Cloud] {
            let data = tempfile::tempdir().unwrap();
            let engine =
                Engine::in_memory(AppPaths::new(data.path()), AiConfig::default()).unwrap();
            let mut draft = kg_engine::Settings {
                tts_backend: backend,
                ..Default::default()
            };
            let mut fonts_ready = false;
            let mut harness = Harness::builder()
                .with_size([900.0, 900.0])
                .build_ui(move |ui| {
                    crate::theme::apply(ui.ctx());
                    // Fonts set in a frame are used from the next one.
                    if !std::mem::replace(&mut fonts_ready, true) {
                        return;
                    }
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        crate::screens::settings::voice_section(
                            ui,
                            &engine,
                            &mut draft,
                            &mut crate::screens::Actions::default(),
                        );
                    });
                });
            harness.run_steps(5);
            let image = harness.render().unwrap();
            if let Some(dir) = &out {
                image
                    .save(dir.join(format!("{locale}-voice-{backend:?}.png")))
                    .unwrap();
            }
        }
    }
    crate::i18n::set(crate::i18n::DEFAULT_LOCALE);
}

/// Long-running content preparation: plan generation on the Edital screen
/// and "prepare everything" on the Plan screen.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn content_preparation() {
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    let data = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(data.path());
    seed(&paths, false);
    crate::i18n::set("pt-BR");
    let mut harness = Harness::builder()
        .with_size([1180.0, 760.0])
        .build_eframe(|cc| KnowGiverApp::with_paths(cc, paths.clone(), AiConfig::default()));
    crate::i18n::set("pt-BR");

    // No server answers, so the job stays on its first topics.
    harness.state_mut().dispatch(Command::PrepareContent(
        crate::screens::handout::prepare_request(),
    ));
    harness.state_mut().ui.screen = Screen::Plan;
    render(&mut harness, out.as_deref(), "pt-BR-plan-preparing");

    if let Ok(engine) = harness.state_mut().engine.as_mut() {
        engine.simulate_plan_generation(kg_engine::Edital {
            file_name: "edital-tjce.pdf".into(),
            pages: 42,
            text: kg_engine::EditalText::from_raw("EDITAL"),
        });
    }
    harness.state_mut().ui.screen = Screen::Edital;
    render(&mut harness, out.as_deref(), "pt-BR-edital-generating");

    harness.state_mut().dispatch(Command::CancelPrepare);
    crate::i18n::set(crate::i18n::DEFAULT_LOCALE);
}

/// A photographed page for the screenshots: lines of "handwriting" on paper.
fn page_jpeg() -> Vec<u8> {
    let (w, h) = (900_u32, 1240_u32);
    let mut img = image::RgbImage::from_pixel(w, h, image::Rgb([238, 234, 224]));
    for line in 0..26 {
        let y = 70 + line * 42;
        img.put_pixel(40, y + 12, image::Rgb([180, 190, 210]));
        for x in 40..w - 40 {
            img.put_pixel(x, y + 14, image::Rgb([200, 208, 222]));
            let word = (x / 23 + line * 7) % 9;
            if word != 0 && line % 9 != 8 {
                let wave = ((x as f32) / 5.0 + line as f32).sin() * 4.0;
                let yy = (y as f32 + wave) as u32;
                for dy in 0..2 {
                    img.put_pixel(x, yy + dy, image::Rgb([40, 52, 92]));
                }
            }
        }
    }
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
        .encode_image(&img)
        .unwrap();
    out
}

/// A proposal with a graded attempt and one waiting for its transcript to
/// be checked. Returns (graded, in review).
fn seed_writing(paths: &AppPaths, english: bool) -> (kg_engine::AttemptId, kg_engine::AttemptId) {
    use kg_engine::{AttemptOrigin, AttemptStatus, ProblemCategory};
    use kg_study::discursive::{edits, parse_manual_rubric};
    use kg_study::{
        ConfirmedTranscript, CriterionScore, Evaluation, Evidence, Legibility, NewPage,
        NewProposal, Problem, Remark, Transcript, TranscriptPage, Uncertain, WritingKind,
    };
    let tr = |pt, en| tr(english, pt, en);
    let mut store = Store::open(&paths.database()).unwrap();
    let plan = store.list_plans().unwrap()[0].id;
    let loaded = store.load_plan(plan).unwrap();
    let crase = loaded.subjects[0].topics[1].id;
    let rights = loaded.subjects[1].topics[1].id;
    let now = chrono::Local::now().naive_local();
    let mut rubric = parse_manual_rubric(tr(
        "Apresentação e estrutura textual: 5\nDesenvolvimento do tema: 15",
        "Presentation and text structure: 5\nDevelopment of the theme: 15",
    ))
    .unwrap();
    rubric.source = kg_study::RubricSource::Edital;
    rubric.evidence = "12.2 Serão avaliados a apresentação e a estrutura textual (5 pontos) e o desenvolvimento do tema (15 pontos).".into();
    let proposal = store
        .insert_proposal(
            plan,
            &NewProposal {
                kind: WritingKind::Essay,
                difficulty: Difficulty::Medium,
                theme: tr(
                    "Participação social e direitos fundamentais",
                    "Civic participation and fundamental rights",
                )
                .into(),
                statement: tr(
                    "Redija um texto dissertativo-argumentativo sobre o papel da participação social na garantia dos direitos fundamentais, apresentando tese, dois argumentos e conclusão.",
                    "Write an argumentative essay on how civic participation helps secure fundamental rights, with a thesis, two arguments and a conclusion.",
                )
                .into(),
                instructions: tr(
                    "Use a norma-padrão e não copie trechos da proposta.",
                    "Use standard written language and do not copy the prompt.",
                )
                .into(),
                topics: vec![rights, crase],
                line_limit: Some(30),
                minutes: None,
                rubric: rubric.clone(),
                expected_points: vec![
                    tr(
                        "Definir participação social e seus canais (conselhos, audiências, voto).",
                        "Define civic participation and its channels (councils, hearings, voting).",
                    )
                    .into(),
                    tr(
                        "Relacionar a participação à efetividade dos direitos.",
                        "Link participation to rights being honoured in practice.",
                    )
                    .into(),
                ],
                model: "glm-5".into(),
                prompt_version: "proposal-3f2a91c0".into(),
            },
            now,
        )
        .unwrap();

    let text = tr(
        "A participação social é condição para que os direitos\nfundamentais saiam do papel. Quando os cidadão\nacompanham políticas públicas, cobram resultados\ne denunciam abusos, o Estado responde melhor.",
        "Civic participation is what takes fundamental rights\noff the page. When citizens follows public\npolicy, ask for results and report abuse,\nthe State answers better.",
    );
    let reading = |page| Transcript {
        pages: vec![TranscriptPage {
            page,
            lines: text.lines().map(str::to_owned).collect(),
            uncertain: vec![Uncertain {
                line: Some(3),
                text: tr("cobram", "ask").into(),
                note: tr("letra borrada", "smudged").into(),
            }],
            legibility: Legibility::Fair,
            warnings: vec![
                tr(
                    "A margem direita está sombreada.",
                    "The right margin is shaded.",
                )
                .into(),
            ],
            retake: None,
        }],
    };
    let photo = paths.data_dir.join("page-demo.jpg");
    std::fs::write(&photo, page_jpeg()).unwrap();
    let page = NewPage {
        original_path: photo.to_string_lossy().into_owned(),
        processed_path: photo.to_string_lossy().into_owned(),
        rotation: 0,
        width: 900,
        height: 1240,
        issues: vec![kg_engine::PageIssue::Blurry],
    };

    let graded = store
        .insert_attempt(
            proposal,
            AttemptOrigin::Photos,
            None,
            AttemptStatus::Draft,
            now,
        )
        .unwrap();
    store.add_page(graded, &page).unwrap();
    store
        .save_transcript(graded, &reading(1), "glm-4.5v", "transcribe-8c1d22aa", now)
        .unwrap();
    let pages = vec![text.to_owned()];
    store
        .confirm_transcript(
            graded,
            &ConfirmedTranscript {
                edits: edits(&reading(1), &pages),
                pages,
            },
            now,
        )
        .unwrap();
    let quote = |q: &str, line| {
        Some(Evidence {
            quote: q.into(),
            page: Some(1),
            line: Some(line),
        })
    };
    let evaluation = Evaluation {
        scores: vec![
            CriterionScore {
                key: "c1".into(),
                score: 3.5,
                justification: tr(
                    "Texto em parágrafo único: falta separar introdução, desenvolvimento e conclusão.",
                    "One single paragraph: introduction, body and conclusion are not separated.",
                )
                .into(),
                evidence: Vec::new(),
            },
            CriterionScore {
                key: "c2".into(),
                score: 11.0,
                justification: tr(
                    "Tese clara e pertinente; o segundo argumento ficou só anunciado.",
                    "Clear, relevant thesis; the second argument is only announced.",
                )
                .into(),
                evidence: vec![Evidence {
                    quote: tr(
                        "A participação social é condição para que os direitos",
                        "Civic participation is what takes fundamental rights",
                    )
                    .into(),
                    page: Some(1),
                    line: Some(1),
                }],
            },
        ],
        rubric,
        strengths: vec![Remark {
            text: tr("Tese explícita logo na primeira frase.", "Thesis stated in the first sentence.").into(),
            evidence: None,
        }],
        problems: vec![Problem {
            category: ProblemCategory::Writing,
            text: tr("Concordância nominal.", "Subject–verb agreement.").into(),
            suggestion: tr("Escreva \"os cidadãos\".", "Write \"citizens follow\".").into(),
            evidence: quote(tr("os cidadão", "citizens follows"), 2),
        }],
        priorities: vec![
            tr("Dividir o texto em parágrafos.", "Split the text into paragraphs.").into(),
            tr("Desenvolver o segundo argumento com um exemplo.", "Develop the second argument with an example.").into(),
            tr("Revisar a concordância.", "Check agreement.").into(),
        ],
        review_topics: vec![crase],
        dropped_quotes: 1,
        model: "glm-5".into(),
        prompt_version: "evaluate-51be07d4".into(),
        evaluated_at: now,
    };
    store.save_evaluation(graded, &evaluation).unwrap();
    store
        .set_attempt_status(graded, AttemptStatus::Evaluated, None, "", now)
        .unwrap();

    let review = store
        .insert_attempt(
            proposal,
            AttemptOrigin::Photos,
            None,
            AttemptStatus::Draft,
            now,
        )
        .unwrap();
    store.add_page(review, &page).unwrap();
    store
        .save_transcript(review, &reading(1), "glm-4.5v", "transcribe-8c1d22aa", now)
        .unwrap();
    store
        .set_attempt_status(review, AttemptStatus::Review, None, "", now)
        .unwrap();
    (graded, review)
}

#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn writing_snapshots() {
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    for locale in ["pt-BR", "en"] {
        let english = locale == "en";
        let data = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(data.path());
        seed(&paths, english);
        let (graded, review) = seed_writing(&paths, english);
        crate::i18n::set(locale);
        let mut harness = Harness::builder()
            .with_size([1180.0, 1500.0])
            .build_eframe(|cc| KnowGiverApp::with_paths(cc, paths.clone(), AiConfig::default()));
        crate::i18n::set(locale);
        harness.state_mut().ui.screen = Screen::Discursive;
        render(&mut harness, out.as_deref(), &format!("{locale}-writing"));
        for (attempt, name) in [(graded, "writing-graded"), (review, "writing-review")] {
            harness.state_mut().dispatch(Command::OpenAttempt(attempt));
            // Page photos load on a worker.
            for _ in 0..100 {
                harness.run_steps(1);
                let loaded = harness.state().engine.as_ref().is_ok_and(|e| {
                    e.open_attempt()
                        .is_some_and(|o| o.pages.iter().all(|p| e.page_image(p.id).is_some()))
                });
                if loaded {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            render(&mut harness, out.as_deref(), &format!("{locale}-{name}"));
            harness.state_mut().dispatch(Command::CloseAttempt);
        }
        harness.state_mut().ui.discursive.history = true;
        render(
            &mut harness,
            out.as_deref(),
            &format!("{locale}-writing-history"),
        );
    }
}
