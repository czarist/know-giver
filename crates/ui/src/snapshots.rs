//! Renders every screen to PNG for visual review. Needs a GPU, so it is
//! ignored by default:
//!
//! KG_SNAPSHOT_DIR=<dir> cargo test -p kg-ui snapshots -- --ignored

use chrono::Days;
use egui_kittest::Harness;
use kg_engine::{AiConfig, AppPaths, Command, Engine};
use kg_study::{Difficulty, NewPlan, NewSubject, NewTopic, ReviewGrade, Store, TopicStatus};

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
            (Screen::Edital, "edital"),
            (Screen::Plan, "plan"),
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
        harness.state_mut().ui.screen = Screen::Lesson;
        render(
            &mut harness,
            out.as_deref(),
            &format!("{locale}-lesson-active"),
        );
        harness.state_mut().dispatch(Command::EndLesson);
    }
    crate::i18n::set(crate::i18n::DEFAULT_LOCALE);
}
