//! Haul's desktop app: Tauri glue around the engine.

pub mod bridge;
pub mod clipboard;
pub mod commands;
pub mod core;
pub mod dto;

use crate::core::AppCore;
use std::time::Duration;
use tauri::{Emitter, Manager, RunEvent};

/// Keeps the log writer alive for the life of the app.
struct LogGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

fn init_logging(dir: &std::path::Path) -> Option<LogGuard> {
    std::fs::create_dir_all(dir).ok()?;
    let (writer, guard) = tracing_appender::non_blocking(tracing_appender::rolling::daily(dir, "haul.log"));
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .with_writer(writer)
        .with_ansi(false)
        .try_init()
        .ok()?;
    Some(LogGuard(guard))
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::add_links,
            commands::pause,
            commands::resume,
            commands::remove,
            commands::pause_all,
            commands::set_priority,
            commands::reorder,
            commands::set_speed_limit,
            commands::open_folder,
            commands::collector_snapshot,
            commands::collector_start,
            commands::collector_remove,
            commands::collector_recheck,
            commands::set_collector_filters,
            commands::set_watch_clipboard,
        ])
        .setup(|app| {
            if let Some(g) = app.path().app_log_dir().ok().and_then(|d| init_logging(&d)) {
                app.manage(g);
            }
            let data_dir = app.path().app_data_dir()?;
            let download_dir =
                app.path().download_dir().or_else(|_| app.path().home_dir().map(|h| h.join("Downloads")))?;
            let db = data_dir.join("haul.db");
            let core = tauri::async_runtime::block_on(async { AppCore::start(&db, download_dir) })?;
            let handle = app.handle().clone();
            let events = core.events();
            tauri::async_runtime::block_on(async move {
                crate::bridge::spawn_forwarder(events, Duration::from_millis(250), move |u| {
                    let _ = handle.emit("haul:update", u);
                });
            });
            tracing::info!(db = %db.display(), "Haul started");
            if let Err(e) = crate::clipboard::spawn_watcher(core.clone()) {
                tracing::warn!(error = %e, "clipboard watcher not started");
            }
            app.manage(core);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Haul");
    app.run(|app, e| {
        if let RunEvent::Exit = e {
            tracing::info!("Haul exiting: saving downloads");
            tauri::async_runtime::block_on(app.state::<AppCore>().shutdown());
        }
    });
}
