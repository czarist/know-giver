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
            "texto",
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
    }
    crate::i18n::set(crate::i18n::DEFAULT_LOCALE);
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
