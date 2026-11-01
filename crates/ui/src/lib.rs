//! egui interface for Know Giver.
//!
//! Presentation only: all state lives in [`kg_engine::Engine`], and user
//! actions are sent to it as [`kg_engine::Command`]s. Every visible string
//! comes from the translation files in `locales/` through the internal `i18n` module.

// Embeds `locales/*.yml` and defines the lookup used by `t!`.
rust_i18n::i18n!("../../locales", fallback = "pt-BR");

mod app;
mod art;
mod export;
mod i18n;
mod icon;
mod icons;
mod illustrations;
mod legal;
mod provider_logo;
mod reminders;
mod screens;
mod teacher;
mod theme;
mod tour;
mod tray;
mod update_bar;
mod whiteboard;
mod widgets;
mod working;

#[cfg(test)]
mod snapshots;

pub use app::KnowGiverApp;
pub use icon::rgba as icon_rgba;

pub use kg_engine::{AiConfig, AppPaths};
use rust_i18n::t;

/// Stable identifier for eframe's settings storage. Not shown to the user.
const APP_ID: &str = "know-giver";

/// Options passed by the installed app's launcher.
#[derive(Debug, Default, Clone, Copy)]
pub struct RunOptions {
    /// Start without showing the main window.
    pub start_hidden: bool,
    /// Enable reminders selected in the installer.
    pub enable_study_reminders: bool,
}

/// Opens Know Giver and runs until the user exits. `env_ai` supplies initial
/// AI settings from `.env`; `run_options` controls installer-selected behavior.
///
/// # Errors
///
/// Returns an error if eframe cannot start the native application.
pub fn run(env_ai: AiConfig, run_options: RunOptions) -> eframe::Result {
    rust_i18n::set_locale(i18n::DEFAULT_LOCALE);
    let title = t!("app.name").into_owned();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&title)
            .with_app_id(APP_ID)
            .with_icon(icon::app_icon())
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([820.0, 520.0])
            .with_visible(!run_options.start_hidden),
        ..Default::default()
    };

    eframe::run_native(
        APP_ID,
        options,
        Box::new(move |cc| {
            Ok(Box::new(KnowGiverApp::with_run_options(
                cc,
                env_ai,
                run_options,
            )))
        }),
    )
}
