//! Know Giver desktop entry point.

// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::MakeWriterExt;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // GPU crates are noisy about third-party overlay layers; keep them quiet
    // unless RUST_LOG asks otherwise.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info,wgpu_hal=error,wgpu_core=warn,egui_wgpu=warn,pdf_extract=error")
    });
    // Also to a file: release builds have no console to read it from.
    let log = log_file();
    let fresh_log = log.as_ref().and_then(|file| file.try_clone().ok());
    let subscriber = tracing_subscriber::fmt().with_env_filter(filter);
    match log {
        Some(file) => subscriber
            .with_ansi(false)
            .with_writer(std::io::stderr.and(std::sync::Mutex::new(file)))
            .init(),
        None => subscriber.init(),
    }

    let args: Vec<_> = std::env::args_os().collect();
    // The study reminder popup, opened by the running copy.
    if args.get(1).and_then(|arg| arg.to_str()) == Some(kg_ui::REMINDER_FLAG) {
        let rest: Vec<String> = args[2..]
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        kg_ui::run_reminder(&rest)?;
        return Ok(());
    }
    let start_hidden = args.iter().any(|arg| arg.to_str() == Some("--background"));
    // Only one copy runs: a later launch shows that copy's window and exits.
    let Some(instance) = kg_ui::claim_instance(start_hidden) else {
        return Ok(());
    };
    // The log is per run; it is only emptied once this copy is the one running.
    if let Some(file) = fresh_log
        && let Err(err) = file.set_len(0)
    {
        tracing::warn!(%err, "could not clear the previous log");
    }

    load_env();
    let enable_study_reminders = args
        .iter()
        .any(|arg| arg.to_str() == Some("--enable-study-reminders"))
        || take_study_reminder_marker();
    kg_ui::run(
        kg_ui::AiConfig::from_env(|key| std::env::var(key).ok()),
        kg_ui::RunOptions {
            start_hidden,
            enable_study_reminders,
        },
        instance,
    )?;
    Ok(())
}

fn take_study_reminder_marker() -> bool {
    let path = match std::env::current_exe() {
        Ok(exe) => exe.parent().map(|dir| dir.join(".study-reminders-enabled")),
        Err(err) => {
            tracing::warn!(%err, "could not locate the installed reminder preference");
            None
        }
    };
    let Some(path) = path else {
        return false;
    };
    match std::fs::remove_file(&path) {
        Ok(()) => true,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => {
            tracing::warn!(%err, path = %path.display(), "could not read the installed reminder preference");
            false
        }
    }
}

/// The log file, when the data folder can be written. Opened for appending,
/// so a launch that hands over to the running copy keeps that copy's log.
fn log_file() -> Option<std::fs::File> {
    let path = kg_ui::AppPaths::detect().log_file();
    let dir = path.parent()?;
    std::fs::create_dir_all(dir).ok()?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
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
