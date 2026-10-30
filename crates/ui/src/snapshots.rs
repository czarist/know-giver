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

/// Demo content in the screenshots' language. Portuguese and English are
/// written in place; Japanese and Chinese come from [`ASIAN_DEMO`], found by
/// the English text.
fn tr(locale: &str, pt: &'static str, en: &'static str) -> &'static str {
    let column = match locale {
        "pt-BR" => return pt,
        "en" => return en,
        "ja" => 0,
        "zh-Hans" => 1,
        _ => 2,
    };
    ASIAN_DEMO
        .iter()
        .find(|row| row.0 == en)
        .map(|row| [row.1, row.2, row.3][column])
        .unwrap_or_else(|| panic!("no {locale} demo text for {en:?}"))
}

/// The demo plan's notice: it has an essay, so written answers are offered.
const NOTICE: &str = "EDITAL
12. DA PROVA DISCURSIVA
12.1 A prova discursiva consistirá na redação de texto dissertativo, de 20 a 30 linhas, valendo 20 pontos.
12.2 Serão avaliados a apresentação e a estrutura textual (5 pontos) e o desenvolvimento do tema (15 pontos).";

fn seed(paths: &AppPaths, locale: &'static str) {
    // Japanese and Chinese students get the English grammar demo, explained
    // in their language.
    let english = locale != "pt-BR";
    let tr = |pt, en| tr(locale, pt, en);
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
    finish_setup(paths, locale);
}

/// Saves the first-run choices, so screenshots open on the screens.
fn finish_setup(paths: &AppPaths, locale: &'static str) {
    let mut engine = Engine::new(paths.clone(), AiConfig::default()).unwrap();
    let mut settings = engine.settings().clone();
    settings.student_name = tr(locale, "Ana", "Anna").into();
    settings.teacher = Some(settings.teacher());
    settings.locale = locale.into();
    settings.region = kg_engine::Region::guess(locale).or(Some(kg_engine::Region::Us));
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
    for locale in crate::i18n::LOCALES {
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
        if let Some(draft) = &mut setup.draft {
            draft.region = kg_engine::Region::guess(locale).or(Some(kg_engine::Region::Us));
        }
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

/// The terms of use drawn at a narrow width: paragraphs and list items
/// must wrap evenly.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn legal_snapshot() {
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    for locale in crate::i18n::LOCALES {
        crate::i18n::set(locale);
        let mut harness = Harness::builder()
            .with_size([620.0, 2400.0])
            .build_ui(move |ui| {
                let ready = egui::Id::new("theme_ready");
                if !ui
                    .ctx()
                    .data(|d| d.get_temp::<bool>(ready))
                    .unwrap_or(false)
                {
                    crate::theme::apply(ui.ctx());
                    ui.ctx().data_mut(|d| d.insert_temp(ready, true));
                    return;
                }
                egui::Frame::new()
                    .fill(crate::theme::palette(ui.ctx()).surface)
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        let region =
                            kg_engine::Region::guess(locale).unwrap_or(kg_engine::Region::Us);
                        crate::legal::show(ui, crate::legal::LegalDoc::Terms.text(locale, region));
                    });
            });
        harness.run_steps(3);
        let image = harness.render().unwrap();
        if let Some(dir) = out.as_deref() {
            image.save(dir.join(format!("{locale}-terms.png"))).unwrap();
        }
    }
}

/// The provider list as it opens, with logos and groups.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn provider_list_snapshot() {
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    crate::i18n::set("pt-BR");
    for (mainland, name) in [(false, "providers"), (true, "providers-mainland")] {
        let mut harness = Harness::builder()
            .with_size([460.0, 1000.0])
            .build_ui(move |ui| {
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
                        crate::screens::setup::provider_list(ui, None, mainland);
                    });
            });
        harness.run_steps(4);
        std::thread::sleep(std::time::Duration::from_millis(150));
        harness.run_steps(4);
        let image = harness.render().unwrap();
        if let Some(dir) = out.as_deref() {
            image.save(dir.join(format!("{name}.png"))).unwrap();
        }
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

    for locale in crate::i18n::LOCALES {
        let data = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(data.path());
        seed(&paths, locale);
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
                    locale,
                    "Olá! Hoje vamos ver quando usar a crase. Repare: ela é a fusão da preposição a com o artigo a. Você já sabe quando ela é obrigatória?",
                    "Hi! Today we look at subject–verb agreement. The verb follows the head of the subject, not the nearest noun. Which is right: “The list of names is” or “are”?",
                ),
                &[
                    tr(locale, "Crase = preposição a + artigo a", "The verb agrees with the head noun"),
                    tr(locale, "Teste: troque por palavra masculina", "Test: drop the “of …” phrase"),
                ],
            );
            // The lesson plan, on its second step.
            engine.simulate_tutor_text(tr(
                locale,
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
                locale,
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
            locale,
            "2.1.1 CARGO 1: AUDITOR FEDERAL DE FINANÇAS E CONTROLE (AFFC) – ESPECIALIDADE: AUDITORIA\n\n\
             2.1.2 CARGO 2: AUDITOR FEDERAL DE FINANÇAS E CONTROLE (AFFC) – ESPECIALIDADE: TECNOLOGIA DA\n\
             INFORMAÇÃO – CIÊNCIA DE DADOS\n\n\
             2.1.3 CARGO 3: AUDITOR FEDERAL DE FINANÇAS E CONTROLE (AFFC) – ESPECIALIDADE: TECNOLOGIA DA\n\
             INFORMAÇÃO – INFRAESTRUTURA DE TI\n",
            ROLES_EN,
        );
        if let Ok(engine) = harness.state_mut().engine.as_mut() {
            engine.simulate_import(kg_engine::Edital {
                file_name: tr(locale, "edital-cgu.pdf", "exam-notice-cgu.pdf").into(),
                pages: 69,
                text: kg_engine::EditalText::from_raw(roles),
            });
        }
        harness.state_mut().ui.prefs.role = tr(
            locale,
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

        reader_snapshots(&mut harness, out.as_deref(), locale, data.path());
    }
    crate::i18n::set(crate::i18n::DEFAULT_LOCALE);
}

/// A generated handout open in the reader, with a highlight and a note, and
/// the handout library.
fn reader_snapshots(
    harness: &mut Harness<'_, KnowGiverApp>,
    out: Option<&std::path::Path>,
    locale: &'static str,
    dir: &std::path::Path,
) {
    use kg_engine::{
        HandoutDocument, HandoutExercise, HandoutLabels, HandoutSection, TopicHandout,
    };
    let tr = |pt, en| tr(locale, pt, en);
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
    // The paragraph highlighted in the reader.
    let highlighted = tr(
        "Use a crase quando o termo regente exige a preposição a e a palavra seguinte aceita o artigo a. O teste mais rápido é trocar a palavra feminina por uma masculina: se aparecer ao, há crase.",
        "Strip the phrases that start with of, with or in. What is left is the head noun, and the verb follows it: the list of names is long, the boxes on the shelf are heavy.",
    );
    let document = HandoutDocument {
        title: tr("Plano TJCE – Técnico Judiciário", "State Court – Court Clerk").into(),
        subtitle: tr("Emprego do sinal indicativo de crase", "Subject–verb agreement").into(),
        generated: tr("Gerada em 28/10/2026", "Generated on Oct 28, 2026").into(),
        lang: locale.into(),
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
                    paragraphs: vec![highlighted.into()],
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
                // Up to the paragraph's end.
                let end = at + highlighted.len();
                let mut chars = 0;
                let last = boxes
                    .iter()
                    .position(|b| {
                        chars += b.text.len();
                        chars >= end
                    })
                    .unwrap_or(boxes.len() - 1);
                Some((page, first, last))
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
    seed(&paths, "pt-BR");
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
fn seed_writing(paths: &AppPaths, locale: &'static str) -> (kg_engine::AttemptId, kg_engine::AttemptId) {
    use kg_engine::{AttemptOrigin, AttemptStatus, ProblemCategory};
    use kg_study::discursive::{edits, parse_manual_rubric};
    use kg_study::{
        ConfirmedTranscript, CriterionScore, Evaluation, Evidence, Legibility, NewPage,
        NewProposal, Problem, Remark, Transcript, TranscriptPage, Uncertain, WritingKind,
    };
    let tr = |pt, en| tr(locale, pt, en);
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
    for locale in crate::i18n::LOCALES {
        let data = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(data.path());
        seed(&paths, locale);
        let (graded, review) = seed_writing(&paths, locale);
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

/// The English demo edital with several roles.
const ROLES_EN: &str = "2.1.1 CARGO 1: FEDERAL FINANCE AND CONTROL AUDITOR – SPECIALTY: AUDIT\n\n2.1.2 CARGO 2: FEDERAL FINANCE AND CONTROL AUDITOR – SPECIALTY: INFORMATION\nTECHNOLOGY – DATA SCIENCE\n\n2.1.3 CARGO 3: FEDERAL FINANCE AND CONTROL AUDITOR – SPECIALTY: INFORMATION\nTECHNOLOGY – IT INFRASTRUCTURE\n";

/// Japanese, Simplified and Traditional Chinese demo text, by the English
/// text it replaces: (English, ja, zh-Hans, zh-Hant). English example
/// sentences stay in English, as the demo plan studies English grammar.
const ASIAN_DEMO: &[(&str, &str, &str, &str)] = &[
    ("State Court – Court Clerk", "裁判所事務官 – 一般職", "法院书记员 – 公务员考试", "法院書記官 – 司法特考"),
    ("Ceará State Court – Court Clerk – Judicial Area", "裁判所職員採用試験 – 裁判所事務官 – 一般職", "省级法院 – 书记员 – 审判辅助岗位", "司法人員特考 – 法院書記官 – 三等"),
    ("Language and Writing", "英語", "英语", "英文"),
    ("Reading comprehension", "長文読解", "阅读理解", "閱讀測驗"),
    ("Subject–verb agreement", "主語と動詞の一致", "主谓一致", "主詞與動詞一致"),
    ("Verbs and prepositions", "動詞と前置詞", "动词与介词", "動詞與介系詞"),
    ("Punctuation", "句読法", "标点符号", "標點符號"),
    ("Constitutional Law", "憲法", "宪法", "憲法"),
    ("Fundamental principles", "基本原理", "基本原则", "基本原則"),
    ("Fundamental rights", "基本的人権", "公民的基本权利", "基本權利"),
    ("The Judiciary", "司法", "审判机关", "司法"),
    ("exam-notice.pdf", "受験案内.pdf", "招考公告.pdf", "考試簡章.pdf"),
    ("Which is right: \"The list of names is\" or \"are\"?", "“The list of names is” と “are”、正しいのはどちら？", "“The list of names is” 和 “are”，哪个正确？", "“The list of names is” 和 “are”，哪個正確？"),
    ("is", "is", "is", "is"),
    ("are", "are", "are", "are"),
    ("The verb agrees with \"list\", the head noun.", "動詞は中心の名詞 “list” に一致する。", "动词与中心词 “list” 保持一致。", "動詞與中心詞 “list” 一致。"),
    ("True", "正", "正确", "正確"),
    ("False", "誤", "错误", "錯誤"),
    ("past-exam-2024.pdf", "過去問-2024.pdf", "2024真题.pdf", "2024考古題.pdf"),
    ("In “The list of candidates were published”, the verb agrees correctly with its subject.", "“The list of candidates were published” では、動詞は主語に正しく一致している。", "在 “The list of candidates were published” 中，动词与主语的一致是正确的。", "在 “The list of candidates were published” 中，動詞與主詞的一致是正確的。"),
    ("In “Neither the judge nor the clerks was present”, the verb should agree with “clerks”.", "“Neither the judge nor the clerks was present” では、動詞は “clerks” に一致させるべきである。", "在 “Neither the judge nor the clerks was present” 中，动词应与 “clerks” 保持一致。", "在 “Neither the judge nor the clerks was present” 中，動詞應與 “clerks” 一致。"),
    ("Under the civil service statute, a public servant may not act as an agent before public offices.", "国家公務員法上、公務員は官公署に対して他人の代理人となってはならない。", "根据《公务员法》，公务员不得在国家机关担任他人的代理人。", "依公務員服務法，公務員不得擔任他人在公務機關的代理人。"),
    ("The civil service statute is not in the exam notice", "国家公務員法は受験案内の範囲外", "《公务员法》不在招考公告范围内", "公務員服務法不在考試簡章範圍內"),
    ("Anna", "さくら", "林晓", "林曉"),
    ("Hi! Today we look at subject–verb agreement. The verb follows the head of the subject, not the nearest noun. Which is right: “The list of names is” or “are”?", "こんにちは！今日は主語と動詞の一致を学びます。動詞は、すぐ前の名詞ではなく主語の中心となる名詞に合わせます。“The list of names is” と “are”、正しいのはどちら？", "你好！今天我们学习主谓一致。动词要和主语的中心词保持一致，而不是和离它最近的名词。“The list of names is” 和 “are”，哪个正确？", "你好！今天我們學習主詞與動詞一致。動詞要和主詞的中心詞一致，而不是和離它最近的名詞。“The list of names is” 和 “are”，哪個正確？"),
    ("The verb agrees with the head noun", "動詞は主語の中心の名詞に一致", "动词与主语中心词保持一致", "動詞與主詞中心詞一致"),
    ("Test: drop the “of …” phrase", "コツ：“of …” を外して考える", "技巧：先去掉 “of …” 短语", "技巧：先拿掉 “of …” 片語"),
    ("ROTEIRO: Find the head noun | Tricky subjects | Collective nouns | Exam traps\nETAPA: 2", "ROTEIRO: 中心の名詞を探す | まぎらわしい主語 | 集合名詞 | 試験のひっかけ\nETAPA: 2", "ROTEIRO: 找出中心词 | 容易混淆的主语 | 集体名词 | 考试陷阱\nETAPA: 2", "ROTEIRO: 找出中心詞 | 容易混淆的主詞 | 集合名詞 | 考試陷阱\nETAPA: 2"),
    ("Well done, you closed the topic!\nFIM", "よくできました。このトピックは完了です！\nFIM", "做得好，这个知识点学完了！\nFIM", "做得好，這個單元完成了！\nFIM"),
    ("2.1.1 CARGO 1: FEDERAL FINANCE AND CONTROL AUDITOR – SPECIALTY: AUDIT\n\n2.1.2 CARGO 2: FEDERAL FINANCE AND CONTROL AUDITOR – SPECIALTY: INFORMATION\nTECHNOLOGY – DATA SCIENCE\n\n2.1.3 CARGO 3: FEDERAL FINANCE AND CONTROL AUDITOR – SPECIALTY: INFORMATION\nTECHNOLOGY – IT INFRASTRUCTURE\n", "2.1.1 CARGO 1: 財務監査官 – 専門：監査\n\n2.1.2 CARGO 2: 財務監査官 – 専門：情報技術・データサイエンス\n\n2.1.3 CARGO 3: 財務監査官 – 専門：情報技術・ITインフラ\n\n", "2.1.1 CARGO 1: 审计署审计员 – 专业：审计\n\n2.1.2 CARGO 2: 审计署审计员 – 专业：信息技术 – 数据科学\n\n2.1.3 CARGO 3: 审计署审计员 – 专业：信息技术 – IT 基础设施\n\n", "2.1.1 CARGO 1: 審計部審計員 – 專業：審計\n\n2.1.2 CARGO 2: 審計部審計員 – 專業：資訊科技 – 資料科學\n\n2.1.3 CARGO 3: 審計部審計員 – 專業：資訊科技 – IT 基礎設施\n\n"),
    ("exam-notice-cgu.pdf", "受験案内-財務監査官.pdf", "招考公告-审计署.pdf", "考試簡章-審計部.pdf"),
    ("Federal Finance and Control Auditor – Specialty: Information Technology – Data Science", "財務監査官 – 専門：情報技術・データサイエンス", "审计署审计员 – 专业：信息技术 – 数据科学", "審計部審計員 – 專業：資訊科技 – 資料科學"),
    ("Study handout", "テキスト", "讲义", "講義"),
    ("Contents", "目次", "目录", "目錄"),
    ("Key points", "要点", "要点", "重點"),
    ("Examples", "例", "例子", "範例"),
    ("Exam pitfalls", "試験のひっかけ", "考试陷阱", "考試陷阱"),
    ("Summary", "まとめ", "小结", "摘要"),
    ("Exercises", "練習問題", "练习", "練習題"),
    ("Answer key", "解答と解説", "答案解析", "解答與解析"),
    ("Picture: Wikipedia", "画像：Wikipedia", "图片：Wikipedia", "圖片：Wikipedia"),
    ("Quick answers", "解答一覧", "速查答案", "答案速查"),
    ("Generated on Oct 28, 2026", "2026年10月28日作成", "生成于 2026年10月28日", "產生於 2026年10月28日"),
    ("A verb agrees with the head of its subject, not with the noun closest to it. Exams test this with long subjects whose last noun has the other number.", "動詞は、すぐ近くの名詞ではなく、主語の中心となる名詞に一致します。試験では、最後の名詞の数が中心の名詞と違う長い主語でよく問われます。", "动词与主语的中心词保持一致，而不是与离它最近的名词一致。考试常用很长的主语来考查，其最后一个名词的单复数与中心词不同。", "動詞與主詞的中心詞一致，而不是與離它最近的名詞一致。考試常用很長的主詞來考，其最後一個名詞的單複數與中心詞不同。"),
    ("Find the head noun", "中心の名詞を探す", "找出中心词", "找出中心詞"),
    ("Strip the phrases that start with of, with or in. What is left is the head noun, and the verb follows it: the list of names is long, the boxes on the shelf are heavy.", "まず of、with、in で始まる句を外してみましょう。残ったものが中心の名詞で、動詞はそれに合わせます。the list of names is long、the boxes on the shelf are heavy のように。", "先去掉以 of、with 或 in 开头的短语，剩下的就是中心词，动词跟着它变：the list of names is long，the boxes on the shelf are heavy。", "先拿掉以 of、with 或 in 開頭的片語，剩下的就是中心詞，動詞跟著它變：the list of names is long，the boxes on the shelf are heavy。"),
    ("Strip the", "まず", "先去掉", "先拿掉"),
    ("Drop the of-phrase to find the head.", "of の句を外すと中心の名詞が見える。", "去掉 of 短语，就能找到中心词。", "拿掉 of 片語，就能找到中心詞。"),
    ("Tricky subjects", "まぎらわしい主語", "容易混淆的主语", "容易混淆的主詞"),
    ("Each and every take a singular verb. Neither ... nor and either ... or agree with the nearer part of the subject.", "each と every は単数の動詞をとります。neither ... nor と either ... or は、動詞に近いほうの主語に一致します。", "each 和 every 后接单数动词。neither ... nor 和 either ... or 与离动词较近的主语保持一致。", "each 和 every 後接單數動詞。neither ... nor 與 either ... or 和離動詞較近的主詞一致。"),
    ("A number of takes a plural verb; the number of takes a singular one.", "a number of は複数扱い、the number of は単数扱い。", "a number of 接复数动词，the number of 接单数动词。", "a number of 接複數動詞，the number of 接單數動詞。"),
    ("Choose the correct sentence.", "正しい文を選びなさい。", "选出正确的句子。", "選出正確的句子。"),
    ("The list of names are long.", "The list of names are long.", "The list of names are long.", "The list of names are long."),
    ("The list of names is long.", "The list of names is long.", "The list of names is long.", "The list of names is long."),
    ("The head noun is list, singular.", "中心の名詞は list で、単数。", "中心词是 list，为单数。", "中心詞是 list，為單數。"),
    ("Shows up in every exam.", "毎回の試験に出る。", "每次考试都会考。", "每次考試都會考。"),
    ("Presentation and text structure: 5\nDevelopment of the theme: 15", "構成と表現: 5\nテーマの展開: 15", "结构与表达: 5\n主题展开: 15", "結構與表達: 5\n主題發揮: 15"),
    ("Civic participation and fundamental rights", "市民参加と基本的人権", "公民参与与基本权利", "公民參與與基本權利"),
    ("Write an argumentative essay on how civic participation helps secure fundamental rights, with a thesis, two arguments and a conclusion.", "市民参加が基本的人権の保障にどう役立つかについて、主張、二つの根拠、結論を示して論述しなさい。", "请以公民参与如何保障基本权利为题，写一篇议论文，包括论点、两个论据和结论。", "請以公民參與如何保障基本權利為題，寫一篇論說文，包含論點、兩個論據與結論。"),
    ("Use standard written language and do not copy the prompt.", "常体で書き、問題文をそのまま写さないこと。", "使用规范书面语，不要照抄题目。", "使用標準書面語，不要照抄題目。"),
    ("Define civic participation and its channels (councils, hearings, voting).", "市民参加とその手段（審議会、公聴会、選挙）を定義する。", "界定公民参与及其渠道（议事会、听证会、选举）。", "界定公民參與及其管道（審議會、公聽會、選舉）。"),
    ("Link participation to rights being honoured in practice.", "参加と権利の実際の保障を結びつける。", "把参与和权利的切实保障联系起来。", "將參與和權利的實際保障連結起來。"),
    ("Civic participation is what takes fundamental rights\noff the page. When citizens follows public\npolicy, ask for results and report abuse,\nthe State answers better.", "市民参加は、基本的人権を紙の上から\n現実のものにする条件である。市民を政策を\n見守り、成果を求め、不正を告発すれば、\n国はよりよく応えるようになる。", "公民参与是让基本权利\n真正落到实处得条件。当公民关注\n公共政策、要求结果、举报滥用时，\n国家就会作出更好的回应。", "公民參與是讓基本權利\n真正落到實處得條件。當公民關注\n公共政策、要求結果、檢舉濫用時，\n國家就會做出更好的回應。"),
    ("ask", "求め", "要求", "要求"),
    ("smudged", "にじんでいる", "字迹模糊", "字跡模糊"),
    ("The right margin is shaded.", "右の余白に影がある。", "右侧页边有阴影。", "右側邊界有陰影。"),
    ("One single paragraph: introduction, body and conclusion are not separated.", "一段落のみで、序論・本論・結論が分かれていない。", "全文只有一段，开头、主体和结尾没有分开。", "全文只有一段，開頭、主體與結尾沒有分開。"),
    ("Clear, relevant thesis; the second argument is only announced.", "主張は明確で適切。二つ目の根拠は示されただけで展開されていない。", "论点清晰切题；第二个论据只提出而未展开。", "論點清楚切題；第二個論據只提出而未展開。"),
    ("Civic participation is what takes fundamental rights", "市民参加は、基本的人権を紙の上から", "公民参与是让基本权利", "公民參與是讓基本權利"),
    ("Thesis stated in the first sentence.", "最初の文で主張を示している。", "第一句就亮明论点。", "第一句就點出論點。"),
    ("Subject–verb agreement.", "助詞の誤り。", "“的”“得”误用。", "「的」「得」誤用。"),
    ("Write \"citizens follow\".", "「市民が政策を」と書く。", "应写作“实处的条件”。", "應寫作「實處的條件」。"),
    ("citizens follows", "市民を政策を", "实处得条件", "實處得條件"),
    ("Split the text into paragraphs.", "段落に分ける。", "分段写作。", "分段書寫。"),
    ("Develop the second argument with an example.", "二つ目の根拠を例で展開する。", "用例子展开第二个论据。", "用例子展開第二個論據。"),
    ("Check agreement.", "助詞を見直す。", "检查“的”“地”“得”的用法。", "檢查「的」「地」「得」的用法。"),
];

/// One entry of each whiteboard kind in a language: formula, diagram,
/// table, timeline, mind map and exam tip.
fn board_demo(locale: &str) -> [&'static str; 6] {
    match locale {
        "pt-BR" => [
            "FORMULA: juros = capital × taxa × tempo",
            "DIAGRAMA: Edital -> Plano de estudo -> Revisão",
            "TABELA: Lei | Vigência | Aprovada por ; PPA | 4 anos | Congresso ; LDO | 1 ano | Congresso ; LOA | 1 ano | Congresso",
            "LINHA DO TEMPO: 1964: Lei 4.320 | 1988: Constituição | 2000: LRF | 2021: Nova Lei de Licitações",
            "MAPA: Orçamento público: PPA | LDO | LOA | Créditos adicionais",
            "DESTAQUE: A LOA não conterá dispositivo estranho à previsão da receita e à fixação da despesa.",
        ],
        "ja" => [
            "FORMULA: 利息 = 元本 × 利率 × 期間",
            "DIAGRAMA: 受験案内 -> 学習プラン -> 復習",
            "TABELA: 区分 | 期間 | 議決 ; 予算 | 1年 | 国会 ; 補正予算 | 年度内 | 国会 ; 決算 | 1年 | 国会に提出",
            "LINHA DO TEMPO: 1889: 大日本帝国憲法 | 1947: 日本国憲法 | 1947: 財政法 | 1999: 情報公開法",
            "MAPA: 国の予算: 一般会計 | 特別会計 | 政府関係機関 | 補正予算",
            "DESTAQUE: 内閣は、毎会計年度の予算を作成し、国会に提出して、その審議を受け議決を経なければならない。",
        ],
        "zh-Hans" => [
            "FORMULA: 利息 = 本金 × 利率 × 时间",
            "DIAGRAMA: 招考公告 -> 学习计划 -> 复习",
            "TABELA: 文件 | 期限 | 审批 ; 五年规划纲要 | 5年 | 全国人大 ; 中央预算 | 1年 | 全国人大 ; 中央决算 | 1年 | 全国人大常委会",
            "LINHA DO TEMPO: 1954: 第一部宪法 | 1982: 现行宪法 | 1994: 预算法 | 2005: 公务员法",
            "MAPA: 政府预算: 一般公共预算 | 政府性基金预算 | 国有资本经营预算 | 社会保险基金预算",
            "DESTAQUE: 经人民代表大会批准的预算，非经法定程序，不得调整。",
        ],
        "zh-Hant" => [
            "FORMULA: 利息 = 本金 × 利率 × 期間",
            "DIAGRAMA: 考試簡章 -> 讀書計畫 -> 複習",
            "TABELA: 項目 | 期間 | 機關 ; 總預算案 | 1年 | 立法院審議 ; 追加預算 | 年度內 | 立法院審議 ; 決算 | 1年 | 審計部審核",
            "LINHA DO TEMPO: 1932: 預算法 | 1947: 憲法施行 | 1991: 增修條文 | 2005: 政府資訊公開法",
            "MAPA: 政府預算: 總預算 | 特別預算 | 追加預算 | 附屬單位預算",
            "DESTAQUE: 行政院應於會計年度開始三個月前，將下年度預算案提出於立法院。",
        ],
        _ => [
            "FORMULA: interest = principal × rate × time",
            "DIAGRAMA: Exam notice -> Study plan -> Review",
            "TABELA: Law | Term | Passed by ; Multi-year plan | 4 years | Congress ; Budget guidelines | 1 year | Congress ; Annual budget | 1 year | Congress",
            "LINHA DO TEMPO: 1964: Budget law | 1988: Constitution | 2000: Fiscal responsibility | 2021: New procurement law",
            "MAPA: Public budget: Multi-year plan | Guidelines | Annual budget | Extra credits",
            "DESTAQUE: The annual budget may only deal with revenue and spending.",
        ],
    }
}

/// The whiteboard's kinds of entries in each language, for the READMEs.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn whiteboard_snapshots() {
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    for locale in crate::i18n::LOCALES {
        crate::i18n::set(locale);
        let entries = board_demo(locale);
        let mut fonts_ready = false;
        let mut harness = Harness::builder()
            .with_size([1000.0, 660.0])
            .build_ui(move |ui| {
                crate::theme::apply(ui.ctx());
                ui.ctx().set_theme(egui::Theme::Light);
                // Fonts set in a frame are used from the next one.
                if !std::mem::replace(&mut fonts_ready, true) {
                    return;
                }
                let palette = crate::theme::palette(ui.ctx());
                for pair in entries.chunks(2) {
                    ui.horizontal(|ui| {
                        for text in pair {
                            let (area, _) = ui
                                .allocate_exact_size(egui::vec2(485.0, 200.0), egui::Sense::hover());
                            crate::whiteboard::paint(
                                ui,
                                &ui.painter_at(area),
                                area.shrink(14.0),
                                &crate::whiteboard::parse(text),
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
            image.save(dir.join(format!("{locale}-whiteboard.png"))).unwrap();
        }
    }
    crate::i18n::set(crate::i18n::DEFAULT_LOCALE);
}

/// The update offer above the Today screen, in each language.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn update_bar_snapshots() {
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    for locale in crate::i18n::LOCALES {
        let data = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(data.path());
        seed(&paths, locale);
        crate::i18n::set(locale);
        let mut harness = Harness::builder()
            .with_size([1180.0, 760.0])
            .build_eframe(|cc| KnowGiverApp::with_paths(cc, paths.clone(), AiConfig::default()));
        crate::i18n::set(locale);
        if let Ok(engine) = harness.state_mut().engine.as_mut() {
            engine.simulate_update("0.33.0");
        }
        harness.state_mut().ui.screen = Screen::Today;
        render(&mut harness, out.as_deref(), &format!("{locale}-update"));
        harness.state_mut().ui.screen = Screen::Settings;
        render(&mut harness, out.as_deref(), &format!("{locale}-settings-about"));
    }
    crate::i18n::set(crate::i18n::DEFAULT_LOCALE);
}
