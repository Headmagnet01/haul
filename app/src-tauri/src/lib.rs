//! Haul's desktop app: Tauri glue around the engine.

pub mod bridge;
pub mod capture;
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

/// The bundled 7-Zip: next to the app when installed; in the source tree for `tauri dev`.
fn seven_zip_path(app: &tauri::App) -> std::path::PathBuf {
    let bundled = app.path().resource_dir().map(|d| d.join("7zip").join("7z.exe")).unwrap_or_default();
    if bundled.is_file() {
        return bundled;
    }
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/7zip/7z.exe");
    if cfg!(debug_assertions) && source.is_file() {
        return source;
    }
    tracing::warn!(path = %bundled.display(), "bundled 7-Zip not found; extraction will fail");
    bundled
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
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
            commands::postproc_runs,
            commands::postproc_process,
            commands::postproc_retry,
            commands::postproc_password,
            commands::set_expected_hash,
            commands::rules_config,
            commands::set_postproc_settings,
            commands::set_categories,
            commands::set_rules,
            commands::try_rules,
            commands::add_password,
            commands::remove_password,
            commands::order_passwords,
        ])
        .setup(|app| {
            if let Some(g) = app.path().app_log_dir().ok().and_then(|d| init_logging(&d)) {
                app.manage(g);
            }
            let data_dir = app.path().app_data_dir()?;
            let download_dir =
                app.path().download_dir().or_else(|_| app.path().home_dir().map(|h| h.join("Downloads")))?;
            let db = data_dir.join("haul.db");
            let notifier = app.handle().clone();
            let deps = AppCore::real_deps(seven_zip_path(app), move |n| {
                use tauri_plugin_notification::NotificationExt;
                match notifier.notification().builder().title(&n.title).body(&n.body).show() {
                    Ok(()) => tracing::info!(title = %n.title, "notification sent"),
                    Err(e) => tracing::warn!(error = %e, "notification failed"),
                }
            });
            let core = tauri::async_runtime::block_on(async { AppCore::start(&db, download_dir, deps) })?;
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
