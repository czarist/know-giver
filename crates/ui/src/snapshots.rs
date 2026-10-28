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

fn seed(paths: &AppPaths) {
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let mut store = Store::open(&paths.database()).unwrap();
    let topic = |title: &str, difficulty, week| NewTopic {
        title: title.into(),
        difficulty,
        week,
    };
    let plan = NewPlan::new(
        "Plano TJCE – Técnico Judiciário",
        "Tribunal de Justiça do Ceará – Técnico Judiciário – Área Judiciária",
        10,
        vec![
            NewSubject {
                name: "Língua Portuguesa".into(),
                topics: vec![
                    topic(
                        "Compreensão e interpretação de textos",
                        Difficulty::Medium,
                        1,
                    ),
                    topic("Emprego do sinal indicativo de crase", Difficulty::Hard, 1),
                    topic("Regência verbal e nominal", Difficulty::Hard, 2),
                    topic("Pontuação", Difficulty::Easy, 3),
                ],
            },
            NewSubject {
                name: "Noções de Direito Constitucional".into(),
                topics: vec![
                    topic("Princípios fundamentais", Difficulty::Easy, 1),
                    topic("Direitos e garantias fundamentais", Difficulty::Hard, 2),
                    topic("Poder Judiciário", Difficulty::Medium, 3),
                ],
            },
        ],
    )
    .unwrap();
    let today = Engine::today();
    let start = today.checked_sub_days(Days::new(3)).unwrap();
    let mut plan = store
        .insert_plan(&plan, "edital-tjce.pdf", "texto", start)
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
        options: vec!["Certo".into(), "Errado".into()],
        correct: Some(0),
        topic_id: topic,
        relevance,
        reason: reason.into(),
    };
    store
        .insert_board_document(
            plan.id,
            DocumentKind::Exam,
            "prova-trf6-2024.pdf",
            "texto",
            &[
                question(12, "A supressão do acento indicativo de crase em “àquilo” manteria a correção gramatical do texto.", Some(crase), Relevance::Relevant, ""),
                question(13, "No trecho “O direito se constitui em esfera especial”, o pronome poderia ser deslocado para depois do verbo.", Some(crase), Relevance::Partial, ""),
                question(17, "Considerando a Lei n.º 8.112/1990, é proibido ao servidor atuar como procurador junto a repartições públicas.", None, Relevance::OutOfScope, "Lei 8.112/1990 não consta no edital"),
            ],
            &[],
            today,
        )
        .unwrap();
}

fn render(harness: &mut Harness<'_, KnowGiverApp>, dir: Option<&std::path::Path>, name: &str) {
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
    let data = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(data.path());
    seed(&paths);

    for locale in ["pt-BR", "en"] {
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
                "Olá! Hoje vamos ver quando usar a crase. Repare: ela é a fusão da preposição a com o artigo a. Você já sabe quando ela é obrigatória?",
                &["Crase = preposição a + artigo a", "Teste: troque por palavra masculina"],
            );
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
        harness.state_mut().dispatch(Command::EndLesson);
    }
    crate::i18n::set(crate::i18n::DEFAULT_LOCALE);
}

/// The teacher in each lesson phase, side by side.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn avatar_states() {
    use kg_engine::LessonPhase;
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    let phases = [
        (LessonPhase::Idle, 0.0, 0.0),
        (LessonPhase::Listening, 0.0, 0.7),
        (LessonPhase::Thinking, 0.0, 0.0),
        (LessonPhase::Speaking, 0.8, 0.0),
    ];
    let mut harness = Harness::builder()
        .with_size([1000.0, 300.0])
        .build_ui(move |ui| {
            crate::theme::apply(ui.ctx());
            let accent = crate::theme::palette(ui.ctx()).accent;
            ui.horizontal(|ui| {
                for (phase, speech, mic) in phases {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(240.0, 280.0), egui::Sense::hover());
                    crate::avatar::paint(
                        ui,
                        rect,
                        crate::avatar::Mood { phase, speech, mic },
                        accent,
                        crate::avatar::Appearance::default(),
                    );
                }
            });
        });
    harness.run_steps(8);
    let image = harness.render().unwrap();
    if let Some(dir) = out {
        image.save(dir.join("avatar-states.png")).unwrap();
    }
}

/// New visual primitives in light and dark themes, without external models or AI.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn roadmap_components() {
    use crate::{avatar, icons, illustrations, whiteboard};
    let out = std::env::var_os("KG_SNAPSHOT_DIR").map(std::path::PathBuf::from);
    for (name, theme) in [("light", egui::Theme::Light), ("dark", egui::Theme::Dark)] {
        let mut harness = Harness::builder()
            .with_size([1040.0, 720.0])
            .build_ui(move |ui| {
                crate::theme::apply(ui.ctx());
                ui.ctx().set_theme(theme);
                egui_extras::install_image_loaders(ui.ctx());
                let palette = crate::theme::palette(ui.ctx());
                ui.horizontal(|ui| {
                    ui.add(icons::logo(48.0, theme == egui::Theme::Dark));
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
                    for (female, minimal) in
                        [(false, false), (true, false), (false, true), (true, true)]
                    {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(240.0, 240.0), egui::Sense::hover());
                        avatar::paint(
                            ui,
                            rect,
                            avatar::Mood {
                                phase: kg_engine::LessonPhase::Idle,
                                speech: 0.0,
                                mic: 0.0,
                            },
                            palette.accent,
                            avatar::Appearance { female, minimal },
                        );
                    }
                });
                ui.horizontal(|ui| {
                    for text in [
                        "FORMULA: juros = capital × taxa × tempo",
                        "DIAGRAMA: Edital -> Plano de estudo -> Revisão",
                    ] {
                        let (area, _) =
                            ui.allocate_exact_size(egui::vec2(485.0, 180.0), egui::Sense::hover());
                        whiteboard::paint(
                            &ui.painter_at(area),
                            area.shrink(14.0),
                            &whiteboard::parse(text),
                            palette.text,
                        );
                    }
                });
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
            let mut harness = Harness::builder()
                .with_size([900.0, 900.0])
                .build_ui(move |ui| {
                    crate::theme::apply(ui.ctx());
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
    seed(&paths);
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
