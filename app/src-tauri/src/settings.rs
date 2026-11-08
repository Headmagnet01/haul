//! Every user setting in one record. Each field is stored under its own key in the
//! `settings` table, so the keys earlier plans wrote keep working.

use crate::dto::SpeedLimit;
use haul_engine::{Schedule, Store};
use serde::{Deserialize, Deserializer, Serialize};
use std::path::{Path, PathBuf};

pub const KEY_DOWNLOAD_DIR: &str = "download_dir";
pub const KEY_MAX_PARALLEL: &str = "max_parallel";
pub const KEY_CONNECTIONS: &str = "connections";
pub const KEY_SPEED_LIMIT: &str = "speed_limit";
pub const KEY_BROWSER_START: &str = "browser_start";
pub const KEY_WATCH_CLIPBOARD: &str = "watch_clipboard";
pub const KEY_SCHEDULE: &str = "schedule";
pub const KEY_CLOSE_TO_TRAY: &str = "close_to_tray";
pub const KEY_AUTOSTART: &str = "autostart";
pub const KEY_NOTIFY_FINISHED: &str = "notify_finished";
pub const KEY_TRAY_HINT_SHOWN: &str = "tray_hint_shown";
pub const KEY_FIRST_DOWNLOAD_SEEN: &str = "first_download_seen";
pub const KEY_CHECK_UPDATES: &str = "check_updates";
/// Internal: when the update check last ran (UTC, RFC 3339). Not part of `Settings`.
pub const KEY_LAST_UPDATE_CHECK: &str = "last_update_check";

pub const MAX_PARALLEL: (usize, usize) = (1, 20);
pub const CONNECTIONS: (u32, u32) = (1, 16);
pub const MIN_LIMIT_BPS: u64 = 1024;

/// Where a download clicked in the browser goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserStart {
    Now,
    Collector,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub download_dir: PathBuf,
    pub max_parallel: usize,
    pub connections: u32,
    pub speed_limit: SpeedLimit,
    pub browser_start: BrowserStart,
    pub watch_clipboard: bool,
    pub schedule: Option<Schedule>,
    pub close_to_tray: bool,
    pub autostart: bool,
    pub notify_finished: bool,
    pub tray_hint_shown: bool,
    pub first_download_seen: bool,
    pub check_updates: bool,
}

/// The fields a save changes; `None` leaves a field alone. For `schedule`,
/// `Some(None)` (JSON `null`) turns the schedule off.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub download_dir: Option<PathBuf>,
    pub max_parallel: Option<usize>,
    pub connections: Option<u32>,
    pub speed_limit: Option<SpeedLimit>,
    pub browser_start: Option<BrowserStart>,
    pub watch_clipboard: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    pub schedule: Option<Option<Schedule>>,
    pub close_to_tray: Option<bool>,
    pub autostart: Option<bool>,
    pub notify_finished: Option<bool>,
    pub tray_hint_shown: Option<bool>,
    pub first_download_seen: Option<bool>,
    pub check_updates: Option<bool>,
}

/// A field that is present, even as `null`, is `Some(..)`.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

impl Settings {
    /// Missing keys take their defaults. `has_downloads` stands in for `first_download_seen`
    /// on a database from before this setting existed.
    pub fn load(store: &Store, default_dir: PathBuf, has_downloads: bool) -> Settings {
        fn get<T: serde::de::DeserializeOwned>(store: &Store, key: &str) -> Option<T> {
            store.get_setting(key).ok().flatten().and_then(|v| serde_json::from_value(v).ok())
        }
        Settings {
            download_dir: get(store, KEY_DOWNLOAD_DIR).unwrap_or(default_dir),
            max_parallel: get::<usize>(store, KEY_MAX_PARALLEL).unwrap_or(3).clamp(MAX_PARALLEL.0, MAX_PARALLEL.1),
            connections: get::<u32>(store, KEY_CONNECTIONS).unwrap_or(8).clamp(CONNECTIONS.0, CONNECTIONS.1),
            speed_limit: get(store, KEY_SPEED_LIMIT).unwrap_or_default(),
            browser_start: get(store, KEY_BROWSER_START).unwrap_or(BrowserStart::Now),
            watch_clipboard: get(store, KEY_WATCH_CLIPBOARD).unwrap_or(false),
            schedule: get::<Option<Schedule>>(store, KEY_SCHEDULE).flatten(),
            close_to_tray: get(store, KEY_CLOSE_TO_TRAY).unwrap_or(true),
            autostart: get(store, KEY_AUTOSTART).unwrap_or(false),
            notify_finished: get(store, KEY_NOTIFY_FINISHED).unwrap_or(true),
            tray_hint_shown: get(store, KEY_TRAY_HINT_SHOWN).unwrap_or(false),
            first_download_seen: get(store, KEY_FIRST_DOWNLOAD_SEEN).unwrap_or(has_downloads),
            check_updates: get(store, KEY_CHECK_UPDATES).unwrap_or(true),
        }
    }

    /// Takes a patch that `validate` already accepted.
    pub fn apply(&mut self, p: &SettingsPatch) {
        fn set<T: Clone>(field: &mut T, v: &Option<T>) {
            if let Some(v) = v {
                *field = v.clone();
            }
        }
        set(&mut self.download_dir, &p.download_dir);
        set(&mut self.max_parallel, &p.max_parallel);
        set(&mut self.connections, &p.connections);
        set(&mut self.speed_limit, &p.speed_limit);
        set(&mut self.browser_start, &p.browser_start);
        set(&mut self.watch_clipboard, &p.watch_clipboard);
        set(&mut self.schedule, &p.schedule);
        set(&mut self.close_to_tray, &p.close_to_tray);
        set(&mut self.autostart, &p.autostart);
        set(&mut self.notify_finished, &p.notify_finished);
        set(&mut self.tray_hint_shown, &p.tray_hint_shown);
        set(&mut self.first_download_seen, &p.first_download_seen);
        set(&mut self.check_updates, &p.check_updates);
    }

    /// Saves the fields `p` sets, with the values this record now holds.
    pub fn save(&self, store: &Store, p: &SettingsPatch) -> Result<(), haul_engine::EngineError> {
        fn put<T: Serialize>(store: &Store, key: &str, v: &T) -> Result<(), haul_engine::EngineError> {
            let v = serde_json::to_value(v).map_err(|e| haul_engine::EngineError::Io(std::io::Error::other(e)))?;
            store.set_setting(key, &v)
        }
        let s = self;
        if p.download_dir.is_some() {
            put(store, KEY_DOWNLOAD_DIR, &s.download_dir)?;
        }
        if p.max_parallel.is_some() {
            put(store, KEY_MAX_PARALLEL, &s.max_parallel)?;
        }
        if p.connections.is_some() {
            put(store, KEY_CONNECTIONS, &s.connections)?;
        }
        if p.speed_limit.is_some() {
            put(store, KEY_SPEED_LIMIT, &s.speed_limit)?;
        }
        if p.browser_start.is_some() {
            put(store, KEY_BROWSER_START, &s.browser_start)?;
        }
        if p.watch_clipboard.is_some() {
            put(store, KEY_WATCH_CLIPBOARD, &s.watch_clipboard)?;
        }
        if p.schedule.is_some() {
            put(store, KEY_SCHEDULE, &s.schedule)?;
        }
        if p.close_to_tray.is_some() {
            put(store, KEY_CLOSE_TO_TRAY, &s.close_to_tray)?;
        }
        if p.autostart.is_some() {
            put(store, KEY_AUTOSTART, &s.autostart)?;
        }
        if p.notify_finished.is_some() {
            put(store, KEY_NOTIFY_FINISHED, &s.notify_finished)?;
        }
        if p.tray_hint_shown.is_some() {
            put(store, KEY_TRAY_HINT_SHOWN, &s.tray_hint_shown)?;
        }
        if p.first_download_seen.is_some() {
            put(store, KEY_FIRST_DOWNLOAD_SEEN, &s.first_download_seen)?;
        }
        if p.check_updates.is_some() {
            put(store, KEY_CHECK_UPDATES, &s.check_updates)?;
        }
        Ok(())
    }
}

/// Clamps numbers into range and refuses what can't be fixed. The message starts with the field name.
pub fn validate(mut p: SettingsPatch) -> Result<SettingsPatch, String> {
    if let Some(dir) = &p.download_dir {
        check_writable(dir).map_err(|e| format!("downloadDir: {e}"))?;
    }
    if let Some(Some(s)) = &p.schedule
        && s.weekdays.is_empty()
    {
        return Err("schedule: choose at least one day".into());
    }
    p.max_parallel = p.max_parallel.map(|n| n.clamp(MAX_PARALLEL.0, MAX_PARALLEL.1));
    p.connections = p.connections.map(|n| n.clamp(CONNECTIONS.0, CONNECTIONS.1));
    if let Some(l) = &mut p.speed_limit
        && l.enabled
    {
        l.bps = l.bps.max(MIN_LIMIT_BPS);
    }
    Ok(p)
}

/// A folder Haul can create files in: it exists, and a probe file can be created and deleted.
pub fn check_writable(dir: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Err("that folder doesn't exist".into());
    }
    let probe = dir.join(".haul-write-test");
    std::fs::write(&probe, b"").map_err(|e| format!("Haul can't save files there ({e})"))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveTime, Weekday};

    fn t(h: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, 0, 0).unwrap()
    }

    #[test]
    fn defaults_when_empty() {
        let s = Settings::load(&Store::open_in_memory().unwrap(), PathBuf::from(r"C:\Dl"), false);
        assert_eq!(s.download_dir, PathBuf::from(r"C:\Dl"));
        assert_eq!(s.max_parallel, 3);
        assert_eq!(s.connections, 8);
        assert_eq!(s.speed_limit, SpeedLimit::default());
        assert_eq!(s.browser_start, BrowserStart::Now);
        assert!(!s.watch_clipboard);
        assert!(s.schedule.is_none());
        assert!(s.close_to_tray && !s.autostart && s.notify_finished);
        assert!(!s.tray_hint_shown && !s.first_download_seen);
    }

    #[test]
    fn check_updates_defaults_on_and_saves() {
        let store = Store::open_in_memory().unwrap();
        let mut s = Settings::load(&store, PathBuf::from("d"), false);
        assert!(s.check_updates);
        let p: SettingsPatch = serde_json::from_str(r#"{"checkUpdates": false}"#).unwrap();
        s.apply(&p);
        s.save(&store, &p).unwrap();
        assert!(!Settings::load(&store, PathBuf::from("d"), false).check_updates);
        assert_eq!(store.get_setting(KEY_CHECK_UPDATES).unwrap(), Some(serde_json::json!(false)));
    }

    #[test]
    fn first_download_seen_follows_existing_downloads() {
        let s = Settings::load(&Store::open_in_memory().unwrap(), PathBuf::from("d"), true);
        assert!(s.first_download_seen);
    }

    #[test]
    fn existing_keys_read_back() {
        let store = Store::open_in_memory().unwrap();
        store.set_setting(KEY_DOWNLOAD_DIR, &serde_json::json!(r"D:\Mine")).unwrap();
        store.set_setting(KEY_MAX_PARALLEL, &serde_json::json!(5)).unwrap();
        store.set_setting(KEY_SPEED_LIMIT, &serde_json::json!({"enabled": true, "bps": 2048})).unwrap();
        store.set_setting(KEY_WATCH_CLIPBOARD, &serde_json::json!(true)).unwrap();
        let s = Settings::load(&store, PathBuf::from("d"), false);
        assert_eq!(s.download_dir, PathBuf::from(r"D:\Mine"));
        assert_eq!(s.max_parallel, 5);
        assert_eq!(s.speed_limit, SpeedLimit { enabled: true, bps: 2048 });
        assert!(s.watch_clipboard);
    }

    #[test]
    fn save_then_load_roundtrips() {
        let store = Store::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::load(&store, PathBuf::from("d"), false);
        let p = validate(SettingsPatch {
            download_dir: Some(dir.path().to_path_buf()),
            connections: Some(4),
            browser_start: Some(BrowserStart::Collector),
            schedule: Some(Some(Schedule { start: t(1), stop: t(7), weekdays: vec![Weekday::Mon] })),
            close_to_tray: Some(false),
            ..Default::default()
        })
        .unwrap();
        s.apply(&p);
        s.save(&store, &p).unwrap();
        let back = Settings::load(&store, PathBuf::from("d"), false);
        assert_eq!(back.download_dir, dir.path());
        assert_eq!(back.connections, 4);
        assert_eq!(back.browser_start, BrowserStart::Collector);
        assert_eq!(back.schedule.as_ref().map(|s| s.stop), Some(t(7)));
        assert!(!back.close_to_tray);
    }

    #[test]
    fn null_schedule_turns_it_off() {
        let p: SettingsPatch = serde_json::from_str(r#"{"schedule": null}"#).unwrap();
        assert!(matches!(p.schedule, Some(None)));
        let p: SettingsPatch = serde_json::from_str(r#"{}"#).unwrap();
        assert!(p.schedule.is_none());
    }

    #[test]
    fn numbers_are_clamped() {
        let p = |mp, c, bps| SettingsPatch {
            max_parallel: Some(mp),
            connections: Some(c),
            speed_limit: Some(SpeedLimit { enabled: true, bps }),
            ..Default::default()
        };
        let lo = validate(p(0, 0, 10)).unwrap();
        assert_eq!((lo.max_parallel, lo.connections, lo.speed_limit.unwrap().bps), (Some(1), Some(1), 1024));
        let hi = validate(p(99, 40, 5000)).unwrap();
        assert_eq!((hi.max_parallel, hi.connections, hi.speed_limit.unwrap().bps), (Some(20), Some(16), 5000));
    }

    #[test]
    fn disabled_limit_keeps_its_value() {
        let p =
            validate(SettingsPatch { speed_limit: Some(SpeedLimit { enabled: false, bps: 0 }), ..Default::default() })
                .unwrap();
        assert_eq!(p.speed_limit.unwrap().bps, 0);
    }

    #[test]
    fn missing_folder_refused() {
        let e = validate(SettingsPatch {
            download_dir: Some(PathBuf::from(r"C:\definitely\not\here\haul")),
            ..Default::default()
        })
        .unwrap_err();
        assert!(e.starts_with("downloadDir:"), "{e}");
    }

    #[test]
    fn schedule_without_days_refused() {
        let e = validate(SettingsPatch {
            schedule: Some(Some(Schedule { start: t(1), stop: t(2), weekdays: vec![] })),
            ..Default::default()
        })
        .unwrap_err();
        assert!(e.starts_with("schedule:"), "{e}");
    }

    #[test]
    fn writable_folder_leaves_no_probe_file() {
        let dir = tempfile::tempdir().unwrap();
        check_writable(dir.path()).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
