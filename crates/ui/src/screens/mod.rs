//! One module per screen. Each reads engine state and collects user actions,
//! which run after drawing; none of them contain business rules.

pub mod board;
pub mod discursive;
pub mod edital;
pub mod handout;
pub mod lesson;
pub mod plan;
pub mod progress;
pub mod quiz;
pub mod reader;
pub mod review;
pub mod settings;
pub mod setup;
pub mod today;
pub mod welcome;

use kg_engine::{Command, Engine, StudyPlan};

use crate::app::{KnowGiverApp, Screen};

/// What the user asked for while a screen was drawn.
#[derive(Debug, Default)]
pub struct Actions {
    commands: Vec<Command>,
    go_to: Option<Screen>,
}

impl Actions {
    pub fn send(&mut self, command: Command) {
        self.commands.push(command);
    }

    pub fn go_to(&mut self, screen: Screen) {
        self.go_to = Some(screen);
    }

    pub fn run(self, app: &mut KnowGiverApp) {
        for command in self.commands {
            app.dispatch(command);
        }
        if let Some(screen) = self.go_to {
            app.go_to(screen);
        }
    }
}

/// Study week of the plan we are in today (1-based).
pub fn current_week(plan: &StudyPlan) -> u32 {
    let days = (Engine::today() - plan.created_on).num_days().max(0);
    ((days / 7) as u32 + 1).clamp(1, plan.weeks.max(1))
}

/// What a topic's lesson button does: continue an unfinished lesson, look
/// back at a taken one, or start one.
pub fn lesson_action(engine: &Engine, topic: &kg_engine::Topic) -> (String, Command) {
    if engine.has_lesson_draft(topic.id) {
        (
            rust_i18n::t!("plan.lesson_continue").into_owned(),
            Command::ResumeLesson(topic.id),
        )
    } else if topic.lesson_taken() {
        (
            rust_i18n::t!("plan.lesson_again").into_owned(),
            Command::OpenRecap(topic.id),
        )
    } else {
        (
            rust_i18n::t!("plan.lesson").into_owned(),
            Command::StartLesson(topic.id),
        )
    }
}

/// A small button running [`lesson_action`]; opens the lesson screen.
pub fn lesson_button(
    ui: &mut egui::Ui,
    engine: &Engine,
    topic: &kg_engine::Topic,
    actions: &mut Actions,
) {
    let (label, command) = lesson_action(engine, topic);
    if ui.small_button(label).clicked() {
        actions.send(command);
        actions.go_to(Screen::Lesson);
    }
}
