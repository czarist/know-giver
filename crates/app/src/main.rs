//! Know Giver desktop entry point.

// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tracing_subscriber::EnvFilter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // GPU crates are noisy about third-party overlay layers; keep them quiet
    // unless RUST_LOG asks otherwise.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info,wgpu_hal=error,wgpu_core=warn,egui_wgpu=warn,pdf_extract=error")
    });
    tracing_subscriber::fmt().with_env_filter(filter).init();

    load_env();
    kg_ui::run(kg_ui::AiConfig::from_env(|key| std::env::var(key).ok()))?;
    Ok(())
}

/// Loads `.env` from the working directory (development) or from next to
/// the executable (installed app). It wins over global variables with the
/// same name, since it is this app's own configuration.
fn load_env() {
    if dotenvy::dotenv_override().is_ok() {
        return;
    }
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(".env")));
    if let Some(path) = beside_exe
        && path.is_file()
        && let Err(err) = dotenvy::from_path_override(&path)
    {
        tracing::warn!(%err, path = %path.display(), "could not read .env");
    }
}
