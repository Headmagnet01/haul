//! Windows toasts with an Open folder button. Clicking the toast shows Haul on Finished.

use haul_engine::postproc::Notice;
use std::path::PathBuf;
use tauri::AppHandle;

/// What one toast shows and does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToastSpec {
    pub title: String,
    pub body: String,
    /// The folder the Open folder button opens; no button when `None`.
    pub open_folder: Option<PathBuf>,
    /// The view a click on the toast shows (`haul:show-view`); `None` just shows the window.
    pub click_view: Option<&'static str>,
}

/// The button's action, sent back to `on_activated`.
const OPEN: &str = "open";

/// Whether a post-processing notice is shown: "is ready" notices follow the switch,
/// "needs attention" notices always show.
pub fn should_show(n: &Notice, notify_finished: bool) -> bool {
    notify_finished || n.title.ends_with("needs attention")
}

/// The toast for a post-processing notice.
pub fn toast_for(n: &Notice) -> ToastSpec {
    ToastSpec {
        title: n.title.clone(),
        body: n.body.clone(),
        open_folder: n.folder.clone(),
        click_view: Some("finished"),
    }
}

/// The AUMID: the app's identifier when installed. A dev build (`target\debug` or
/// `target\release`) has no registered AUMID, so Windows would drop its toasts; it borrows
/// PowerShell's, as `tauri-plugin-notification` does.
#[cfg(windows)]
fn app_id(app: &AppHandle) -> String {
    let dev = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.to_path_buf()))
        .is_some_and(|d| d.ends_with("target/debug") || d.ends_with("target/release"));
    if dev { tauri_winrt_notification::Toast::POWERSHELL_APP_ID.to_string() } else { app.config().identifier.clone() }
}

/// A click: the button opens the folder; the toast itself shows Haul (on `view`).
fn activated(app: &AppHandle, action: Option<&str>, folder: Option<&std::path::Path>, view: Option<&'static str>) {
    use tauri::Emitter;
    use tauri_plugin_opener::OpenerExt;
    if let (Some(OPEN), Some(dir)) = (action, folder) {
        if let Err(e) = app.opener().open_path(dir.to_string_lossy(), None::<&str>) {
            tracing::warn!(error = %e, "toast: open folder failed");
        }
        return;
    }
    crate::tray::show_main(app);
    if let Some(v) = view {
        let _ = app.emit("haul:show-view", v);
    }
}

/// Shows a toast. Failures are logged, never raised.
#[cfg(windows)]
pub fn show(app: &AppHandle, spec: ToastSpec) {
    use tauri_winrt_notification::Toast;
    let mut t = Toast::new(&app_id(app)).title(&spec.title);
    if !spec.body.is_empty() {
        t = t.text1(&spec.body);
    }
    if spec.open_folder.is_some() {
        t = t.add_button("Open folder", OPEN);
    }
    let (h, folder, view) = (app.clone(), spec.open_folder.clone(), spec.click_view);
    t = t.on_activated(move |action| {
        activated(&h, action.as_deref(), folder.as_deref(), view);
        Ok(())
    });
    match t.show() {
        Ok(()) => tracing::info!(title = %spec.title, "notification sent"),
        Err(e) => tracing::warn!(error = %e, "notification failed"),
    }
}

#[cfg(not(windows))]
pub fn show(_app: &AppHandle, spec: ToastSpec) {
    tracing::info!(title = %spec.title, "notification skipped: not Windows");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(title: &str, folder: Option<&str>) -> Notice {
        Notice { title: title.into(), body: "Extracted 3 files".into(), folder: folder.map(PathBuf::from) }
    }

    #[test]
    fn ready_notice_has_open_folder() {
        let s = toast_for(&notice("movie.7z is ready", Some(r"C:\Downloads\movie")));
        assert_eq!(s.title, "movie.7z is ready");
        assert_eq!(s.body, "Extracted 3 files");
        assert_eq!(s.open_folder, Some(PathBuf::from(r"C:\Downloads\movie")));
        assert_eq!(s.click_view, Some("finished"));
    }

    #[test]
    fn attention_notice_ignores_switch() {
        let ready = notice("a.zip is ready", Some(r"C:\D"));
        let attention = notice("a.zip needs attention", Some(r"C:\D"));
        assert!(should_show(&ready, true));
        assert!(!should_show(&ready, false));
        assert!(should_show(&attention, true));
        assert!(should_show(&attention, false));
    }

    #[test]
    fn no_folder_no_button() {
        let s = toast_for(&notice("a.zip is ready", None));
        assert_eq!(s.open_folder, None);
    }
}
