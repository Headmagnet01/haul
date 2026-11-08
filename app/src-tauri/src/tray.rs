//! The tray icon: its tooltip, its menu, and showing the window.

use haul_engine::{DownloadState, Event};
use std::collections::HashMap;

/// What the tray shows, kept up to date from engine events.
#[derive(Default)]
pub struct TrayStats {
    /// Running downloads and their latest speed.
    running: HashMap<i64, u64>,
}

impl TrayStats {
    pub fn on_event(&mut self, e: &Event) {
        match e {
            Event::StateChanged { id, state: DownloadState::Running, .. } => {
                self.running.entry(*id).or_insert(0);
            }
            Event::StateChanged { id, .. } | Event::Removed { id } => {
                self.running.remove(id);
            }
            Event::Progress { id, speed_bps, .. } => {
                if let Some(s) = self.running.get_mut(id) {
                    *s = *speed_bps;
                }
            }
            _ => {}
        }
    }

    pub fn busy(&self) -> bool {
        !self.running.is_empty()
    }

    /// `Haul · 2 downloading · 7.0 MB/s`, or `Haul · idle`.
    pub fn tooltip(&self) -> String {
        if self.running.is_empty() {
            return "Haul · idle".into();
        }
        let total: u64 = self.running.values().sum();
        let n = self.running.len();
        if total == 0 {
            format!("Haul · {n} downloading")
        } else {
            format!("Haul · {n} downloading · {}", format_speed(total))
        }
    }
}

/// Speed as the status bar writes it (decimal units).
pub fn format_speed(bps: u64) -> String {
    let b = bps as f64;
    if bps < 1000 {
        format!("{bps} B/s")
    } else if b < 999_500.0 {
        format!("{} KB/s", (b / 1e3).round())
    } else if b < 999_950_000.0 {
        format!("{:.1} MB/s", b / 1e6)
    } else {
        format!("{:.2} GB/s", b / 1e9)
    }
}

/// True when Haul was started at sign-in and should stay in the tray.
pub fn starts_hidden(args: &[String]) -> bool {
    args.iter().any(|a| a == "--hidden")
}

/// A second launch shows the running window, except one from sign-in (`--hidden`).
pub fn should_show(args: &[String]) -> bool {
    !starts_hidden(args)
}

/// The dark theme's signal orange, `#ff7b33`: it reads on both light and dark taskbars.
const DOT: [u8; 4] = [0xff, 0x7b, 0x33, 0xff];

/// Paints a dot in the bottom-right corner of an RGBA image: "downloading".
pub fn with_dot(rgba: &mut [u8], w: u32, h: u32) {
    let r = w.min(h) as f32 * 0.2;
    let (cx, cy) = (w as f32 - r - 1.0, h as f32 - r - 1.0);
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            if dx * dx + dy * dy <= r * r {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&DOT);
            }
        }
    }
}

pub const TRAY_ID: &str = "haul";

/// Shows, restores and focuses the main window.
pub fn show_main(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Builds the tray icon and its menu, and keeps the tooltip and icon current.
pub fn build_tray(app: &tauri::AppHandle, core: &crate::core::AppCore) -> tauri::Result<()> {
    use tauri::image::Image;
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri::{Emitter, Manager};

    let base = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;
    let idle = base.clone().to_owned();
    let mut px = base.rgba().to_vec();
    with_dot(&mut px, base.width(), base.height());
    let active = Image::new_owned(px, base.width(), base.height());

    let none = None::<&str>;
    let limit_on = core.settings().speed_limit.enabled;
    let limit = CheckMenuItem::with_id(app, "speed_limit", "Speed limit", true, limit_on, none)?;
    let wd = [
        CheckMenuItem::with_id(app, "wd_nothing", "Nothing", true, true, none)?,
        CheckMenuItem::with_id(app, "wd_sleep", "Sleep", true, false, none)?,
        CheckMenuItem::with_id(app, "wd_shutdown", "Shut down", true, false, none)?,
    ];
    let when = Submenu::with_items(app, "When done", true, &[&wd[0], &wd[1], &wd[2]])?;
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", "Open Haul", true, none)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "pause_all", "Pause all", true, none)?,
            &MenuItem::with_id(app, "resume_all", "Resume all", true, none)?,
            &limit,
            &when,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit Haul", true, none)?,
        ],
    )?;
    app.manage(TrayMenu { limit: limit.clone(), when_done: wd.clone() });

    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(idle.clone())
        .tooltip("Haul · idle")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| {
            let core = app.state::<crate::core::AppCore>();
            match e.id().as_ref() {
                "open" => show_main(app),
                "pause_all" => core.pause_all(),
                "resume_all" => core.resume_all(),
                "speed_limit" => {
                    let mut l = core.settings().speed_limit;
                    l.enabled = !l.enabled;
                    let patch = crate::settings::SettingsPatch { speed_limit: Some(l), ..Default::default() };
                    if let Err(e) = core.set_settings(patch) {
                        tracing::warn!(error = %e, "tray speed limit toggle failed");
                    }
                    sync_limit(app, l.enabled);
                    let _ = app.emit("haul:update", crate::dto::Update { resync: true, ..Default::default() });
                }
                id @ ("wd_nothing" | "wd_sleep" | "wd_shutdown") => {
                    use crate::when_done::Mode;
                    let mode = match id {
                        "wd_sleep" => Mode::Sleep,
                        "wd_shutdown" => Mode::Shutdown,
                        _ => Mode::Nothing,
                    };
                    publish_when_done(app, core.set_when_done(mode));
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    let mut rx = core.events().subscribe();
    tauri::async_runtime::spawn(async move {
        use tokio::sync::broadcast::error::RecvError;
        let mut stats = TrayStats::default();
        let (mut shown_tip, mut shown_busy) = (String::new(), false);
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tokio::select! {
                ev = rx.recv() => match ev {
                    Ok(e) => stats.on_event(&e),
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                },
                _ = tick.tick() => {
                    let tip = stats.tooltip();
                    if tip != shown_tip {
                        let _ = tray.set_tooltip(Some(&tip));
                        shown_tip = tip;
                    }
                    if stats.busy() != shown_busy {
                        shown_busy = stats.busy();
                        let _ = tray.set_icon(Some(if shown_busy { active.clone() } else { idle.clone() }));
                    }
                }
            }
        }
    });
    Ok(())
}

/// Menu items whose checks follow state changed elsewhere (the status bar, the countdown).
pub struct TrayMenu {
    pub limit: tauri::menu::CheckMenuItem<tauri::Wry>,
    /// Nothing, Sleep, Shut down.
    pub when_done: [tauri::menu::CheckMenuItem<tauri::Wry>; 3],
}

/// Ticks the tray's Speed limit item.
pub fn sync_limit(app: &tauri::AppHandle, on: bool) {
    use tauri::Manager;
    if let Some(m) = app.try_state::<TrayMenu>() {
        let _ = m.limit.set_checked(on);
    }
}

/// Ticks the tray's When done radio items and tells the UI (`haul:when-done`).
pub fn publish_when_done(app: &tauri::AppHandle, s: crate::when_done::Status) {
    use crate::when_done::Mode;
    use tauri::{Emitter, Manager};
    if let Some(m) = app.try_state::<TrayMenu>() {
        for (item, mode) in m.when_done.iter().zip([Mode::Nothing, Mode::Sleep, Mode::Shutdown]) {
            let _ = item.set_checked(s.mode == mode);
        }
    }
    let _ = app.emit("haul:when-done", s);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(id: i64, state: DownloadState) -> Event {
        Event::StateChanged { id, state, error: None }
    }

    fn progress(id: i64, speed_bps: u64) -> Event {
        Event::Progress { id, written: 0, size: None, speed_bps, segments: vec![] }
    }

    #[test]
    fn tooltip_idle() {
        let t = TrayStats::default();
        assert_eq!(t.tooltip(), "Haul · idle");
        assert!(!t.busy());
    }

    #[test]
    fn tooltip_two_running() {
        let mut t = TrayStats::default();
        t.on_event(&state(1, DownloadState::Running));
        t.on_event(&state(2, DownloadState::Running));
        t.on_event(&progress(1, 3_000_000));
        t.on_event(&progress(2, 4_000_000));
        assert_eq!(t.tooltip(), "Haul · 2 downloading · 7.0 MB/s");
        assert!(t.busy());
    }

    #[test]
    fn completed_leaves_count() {
        let mut t = TrayStats::default();
        t.on_event(&state(1, DownloadState::Running));
        t.on_event(&state(2, DownloadState::Running));
        t.on_event(&progress(1, 3_000_000));
        t.on_event(&state(1, DownloadState::Completed));
        t.on_event(&Event::Removed { id: 2 });
        assert_eq!(t.tooltip(), "Haul · idle");
    }

    #[test]
    fn progress_for_a_stopped_download_is_ignored() {
        let mut t = TrayStats::default();
        t.on_event(&progress(9, 1_000));
        assert_eq!(t.tooltip(), "Haul · idle");
    }

    #[test]
    fn one_running_no_speed_yet() {
        let mut t = TrayStats::default();
        t.on_event(&state(1, DownloadState::Running));
        assert_eq!(t.tooltip(), "Haul · 1 downloading");
    }

    #[test]
    fn speed_units() {
        assert_eq!(format_speed(512), "512 B/s");
        assert_eq!(format_speed(45_000), "45 KB/s");
        assert_eq!(format_speed(7_000_000), "7.0 MB/s");
        assert_eq!(format_speed(1_200_000_000), "1.20 GB/s");
    }

    #[test]
    fn dot_is_drawn_bottom_right_only() {
        let (w, h) = (32u32, 32u32);
        let mut px = vec![0u8; (w * h * 4) as usize];
        with_dot(&mut px, w, h);
        let at = |x: u32, y: u32| &px[((y * w + x) * 4) as usize..((y * w + x) * 4 + 4) as usize];
        assert_eq!(at(26, 26), &[0xff, 0x7b, 0x33, 0xff]);
        assert_eq!(at(4, 4), &[0, 0, 0, 0]);
    }

    #[test]
    fn second_launch_args_hidden_keeps_window_hidden() {
        assert!(!should_show(&["haul.exe".into(), "--hidden".into()]));
        assert!(should_show(&["haul.exe".into()]));
    }

    #[test]
    fn hidden_flag_parsed() {
        assert!(starts_hidden(&["haul.exe".into(), "--hidden".into()]));
        assert!(!starts_hidden(&["haul.exe".into()]));
    }
}
