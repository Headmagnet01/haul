//! Haul's desktop app: Tauri glue around the engine.

pub mod api;
pub mod autostart;
pub mod bridge;
pub mod capture;
pub mod clipboard;
pub mod commands;
pub mod core;
pub mod dto;
pub mod settings;
pub mod toast;
pub mod tray;
pub mod update;
pub mod when_done;

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
    let spec = crate::toast::ToastSpec {
        title: "Haul is still running".into(),
        body: "Downloads continue in the tray. Right-click the tray icon to quit.".into(),
        open_folder: None,
        click_view: None,
    };
    crate::toast::show(app, spec);
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

/// Decides "when done" once a second (and arms it as downloads start); shows the window when
/// the countdown starts and publishes every change to the UI and the tray.
fn spawn_when_done(app: tauri::AppHandle, core: AppCore) {
    use crate::when_done::Step;
    use haul_engine::{DownloadState, Event};
    use tokio::sync::broadcast::error::RecvError;
    let mut rx = core.events().subscribe();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        let mut last = core.when_done_status();
        loop {
            tokio::select! {
                ev = rx.recv() => match ev {
                    Ok(Event::StateChanged { state: DownloadState::Running, .. }) => core.when_done_started(),
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                },
                _ = tick.tick() => match core.when_done_step() {
                    Step::Nothing => {}
                    Step::Started => crate::tray::show_main(&app),
                    Step::Fire(mode) => {
                        crate::tray::publish_when_done(&app, core.when_done_status());
                        tracing::info!(?mode, "when done: carrying out");
                        if let Err(e) = core.carry_out(mode).await {
                            tracing::warn!(error = %e, "when done failed");
                            crate::toast::show(&app, crate::toast::when_done_failed(&e.to_string()));
                        }
                    }
                },
            }
            let s = core.when_done_status();
            if s != last {
                crate::tray::publish_when_done(&app, s);
                last = s;
            }
        }
    });
}

/// Rewrites the Start with Windows value quoted for this exe, or turns the setting off if the value is gone.
fn repair_autostart(core: &AppCore) {
    let on = core.settings().autostart;
    let result = std::env::current_exe()
        .and_then(|exe| crate::autostart::repair(&crate::autostart::Autostart::real(), on, &exe));
    match result {
        Ok(now) if now != on => {
            tracing::info!("autostart: the Run value is gone; turning the setting off");
            let patch = crate::settings::SettingsPatch { autostart: Some(now), ..Default::default() };
            if let Err(e) = core.set_settings(patch) {
                tracing::warn!(error = %e, "autostart setting not updated");
            }
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "autostart: could not check the Run value"),
    }
}

pub fn run() {
    let app = tauri::Builder::default()
        // First, so a second launch hands over before anything else starts.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if crate::tray::should_show(&args) {
                crate::tray::show_main(app);
            }
        }))
        .plugin(tauri_plugin_opener::init())
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
            commands::when_done,
            commands::set_when_done,
            commands::when_done_cancel,
            commands::when_done_now,
            commands::collector_snapshot,
            commands::collector_start,
            commands::collector_remove,
            commands::collector_recheck,
            commands::set_collector_filters,
            commands::settings,
            commands::set_settings,
            commands::open_path,
            commands::open_extension_folder,
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
            commands::update_status,
            commands::check_updates_now,
            commands::open_release,
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
                let on = notifier.try_state::<AppCore>().is_none_or(|c| c.settings().notify_finished);
                if crate::toast::should_show(&n, on) {
                    crate::toast::show(&notifier, crate::toast::toast_for(&n));
                } else {
                    tracing::info!(title = %n.title, "notification off");
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
            spawn_when_done(app.handle().clone(), core.clone());
            repair_autostart(&core);
            app.manage(crate::update::Updater::new(crate::update::FEED, env!("CARGO_PKG_VERSION")));
            crate::update::spawn_loop(app.handle().clone(), core.clone());
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
