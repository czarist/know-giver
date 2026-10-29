//! One module per screen. Each reads engine state and collects user actions,
//! which run after drawing; none of them contain business rules.

pub mod board;
pub mod edital;
pub mod handout;
pub mod lesson;
pub mod plan;
pub mod progress;
pub mod quiz;
pub mod reader;
pub mod review;
pub mod settings;
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
