//! Haul's desktop app: Tauri glue around the engine.

pub mod api;
pub mod bridge;
pub mod capture;
pub mod clipboard;
pub mod commands;
pub mod core;
pub mod dto;
pub mod settings;
pub mod tray;

use crate::api::pairing::PairGate;
use crate::api::server::{ApiHandle, DEFAULT_PORTS, OnPair, PairEvent, start_api};
use crate::core::AppCore;
use std::sync::Arc;
use std::time::Duration;

/// How long a pairing request waits for Allow or Deny.
const PAIR_TIMEOUT: Duration = Duration::from_secs(60);
use tauri::{Emitter, Manager, RunEvent};

/// The one-time notice that closing the window didn't quit Haul.
fn notify_tray_hint(app: &tauri::AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    let n = app
        .notification()
        .builder()
        .title("Haul is still running")
        .body("Downloads continue in the tray. Right-click the tray icon to quit.");
    if let Err(e) = n.show() {
        tracing::warn!(error = %e, "tray notice failed");
    }
}

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
        // First, so a second launch hands over before anything else starts.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| crate::tray::show_main(app)))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::add_links,
            commands::pause,
            commands::resume,
            commands::remove,
            commands::pause_all,
            commands::set_priority,
            commands::reorder,
            commands::open_folder,
            commands::open_file,
            commands::files_exist,
            commands::delete_files,
            commands::download_again,
            commands::collector_snapshot,
            commands::collector_start,
            commands::collector_remove,
            commands::collector_recheck,
            commands::set_collector_filters,
            commands::settings,
            commands::set_settings,
            commands::open_path,
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
            commands::pair_answer,
            commands::paired_clients,
            commands::remove_paired_client,
        ])
        .on_window_event(|w, e| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                let app = w.app_handle();
                let Some(core) = app.try_state::<AppCore>() else { return };
                let s = core.settings();
                if !s.close_to_tray {
                    return;
                }
                api.prevent_close();
                let _ = w.hide();
                if !s.tray_hint_shown {
                    crate::notify_tray_hint(app);
                    let patch = crate::settings::SettingsPatch { tray_hint_shown: Some(true), ..Default::default() };
                    let _ = core.set_settings(patch);
                }
            }
        })
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
            let gate = PairGate::new(PAIR_TIMEOUT);
            let window = app.handle().clone();
            let on_pair: OnPair = Arc::new(move |ev| match ev {
                PairEvent::Asked { n, browser } => {
                    if let Some(w) = window.get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                    let _ = window.emit("haul:pair-request", serde_json::json!({ "n": n, "browser": browser }));
                }
                PairEvent::Closed { n } => {
                    let _ = window.emit("haul:pair-closed", serde_json::json!({ "n": n }));
                }
            });
            let port_file = data_dir.join("api-port");
            let (c, g) = (core.clone(), gate.clone());
            match tauri::async_runtime::block_on(async move {
                start_api(c, g, on_pair, DEFAULT_PORTS, Some(port_file)).await
            }) {
                Ok(api) => {
                    app.manage(api);
                }
                Err(e) => tracing::warn!(error = %e, "local API not started; the browser extension can't reach Haul"),
            }
            if let Err(e) = crate::tray::build_tray(app.handle(), &core) {
                tracing::warn!(error = %e, "tray icon not created");
            }
            app.manage(gate);
            app.manage(core);
            let args: Vec<String> = std::env::args().collect();
            if !crate::tray::starts_hidden(&args) {
                crate::tray::show_main(app.handle());
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Haul");
    app.run(|app, e| {
        if let RunEvent::Exit = e {
            if let Some(api) = app.try_state::<ApiHandle>() {
                api.stop();
            }
            tracing::info!("Haul exiting: saving downloads");
            tauri::async_runtime::block_on(app.state::<AppCore>().shutdown());
        }
    });
}
