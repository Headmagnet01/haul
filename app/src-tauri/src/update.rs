//! The daily update check: asks GitHub for Haul's latest release and says when it is newer.
//! Nothing is downloaded or installed; the user opens the release page.
//!
//! One request, `GET FEED` with a `User-Agent` and an `Accept` header, and nothing else is sent.

use crate::settings::KEY_LAST_UPDATE_CHECK;
use chrono::{DateTime, Utc};
use haul_engine::Store;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Duration;

pub const FEED: &str = "https://api.github.com/repos/meraaki410/haul/releases/latest";
/// Where "Update available" goes when GitHub's reply names no page of ours.
pub const RELEASES: &str = "https://github.com/meraaki410/haul/releases";
const TIMEOUT: Duration = Duration::from_secs(15);
const EVERY: chrono::TimeDelta = chrono::TimeDelta::hours(24);

/// What the last check found. `latest` and `url` are set only when a newer release exists.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current: String,
    pub latest: Option<String>,
    pub url: Option<String>,
    pub checked_at: Option<String>,
    pub error: Option<String>,
}

/// `v1.2.3` or `1.2.3`: exactly three numbers. A pre-release (`-beta`) or anything else is `None`.
fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim().trim_start_matches('v').split('.').map(|p| p.parse::<u64>().ok());
    let r = (it.next()??, it.next()??, it.next()??);
    it.next().is_none().then_some(r)
}

/// True when `tag` is a plain release version above `current`.
pub fn is_newer(current: &str, tag: &str) -> bool {
    matches!((parse(current), parse(tag)), (Some(c), Some(t)) if t > c)
}

/// `html_url` when it is a page under Haul's own releases, else the releases page. The URL is
/// parsed (so `..` and `%2e%2e` are resolved) and must be https on exactly github.com, with no
/// user name, under `/meraaki410/haul/releases/`.
pub fn safe_release_url(html_url: Option<&str>) -> String {
    let ours = |raw: &str| {
        let u = reqwest::Url::parse(raw).ok()?;
        let ok = u.scheme() == "https"
            && u.host_str() == Some("github.com")
            && u.port().is_none()
            && u.username().is_empty()
            && u.password().is_none()
            && u.path().starts_with("/meraaki410/haul/releases/");
        ok.then(|| u.to_string())
    };
    html_url.and_then(ours).unwrap_or_else(|| RELEASES.to_string())
}

/// True when no check has run yet, the saved time is unreadable, or it is 24 h or more ago.
pub fn due(last: Option<&str>, now: DateTime<Utc>) -> bool {
    match last.and_then(|l| DateTime::parse_from_rfc3339(l).ok()) {
        Some(t) => now - t.with_timezone(&Utc) >= EVERY,
        None => true,
    }
}

/// The update check's HTTP client: rustls (ring), 15 s timeout, `User-Agent: Haul/<version>`.
pub fn client() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(format!("Haul/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("the update client builds")
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: Option<String>,
}

/// Asks `feed` once. Never fails: problems end up in `error`. A 404 (no release yet) is up to date.
pub async fn check(client: &reqwest::Client, feed: &str, current: &str) -> UpdateStatus {
    let mut s =
        UpdateStatus { current: current.to_string(), checked_at: Some(Utc::now().to_rfc3339()), ..Default::default() };
    match ask(client, feed).await {
        Ok(Some(r)) if is_newer(current, &r.tag_name) => {
            s.latest = parse(&r.tag_name).map(|(a, b, c)| format!("{a}.{b}.{c}"));
            s.url = Some(safe_release_url(r.html_url.as_deref()));
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(error = %e, "update check failed");
            s.error = Some(e);
        }
    }
    s
}

async fn ask(client: &reqwest::Client, feed: &str) -> Result<Option<Release>, String> {
    let reply =
        client.get(feed).header(reqwest::header::ACCEPT, "application/vnd.github+json").send().await.map_err(|e| {
            if e.is_timeout() {
                "GitHub didn't answer in time".to_string()
            } else {
                "couldn't reach GitHub".to_string()
            }
        })?;
    let status = reply.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(format!("GitHub answered {}", status.as_u16()));
    }
    let body = reply.bytes().await.map_err(|_| "couldn't read GitHub's reply".to_string())?;
    serde_json::from_slice::<Release>(&body).map(Some).map_err(|_| "couldn't read GitHub's reply".to_string())
}

/// When the check last ran, as saved in the settings table.
pub fn last_check(store: &Store) -> Option<String> {
    store.get_setting(KEY_LAST_UPDATE_CHECK).ok().flatten().and_then(|v| v.as_str().map(str::to_string))
}

pub fn save_last_check(store: &Store, at: DateTime<Utc>) {
    if let Err(e) = store.set_setting(KEY_LAST_UPDATE_CHECK, &serde_json::json!(at.to_rfc3339())) {
        tracing::warn!(error = %e, "update check time not saved");
    }
}

/// The setting that keeps a found update across restarts: `{"latest": "X.Y.Z", "url": "https://…"}`.
pub const KEY_UPDATE_FOUND: &str = "update_found";

/// The saved found update, if any: `(latest, url)`.
pub fn saved_found(store: &Store) -> Option<(String, String)> {
    let v = store.get_setting(KEY_UPDATE_FOUND).ok().flatten()?;
    Some((v.get("latest")?.as_str()?.to_string(), v.get("url")?.as_str()?.to_string()))
}

/// Saves (or, with `None`, clears) the found update.
fn save_found(store: &Store, found: Option<(&str, &str)>) {
    let v = found.map_or(serde_json::Value::Null, |(latest, url)| serde_json::json!({ "latest": latest, "url": url }));
    if let Err(e) = store.set_setting(KEY_UPDATE_FOUND, &v) {
        tracing::warn!(error = %e, "found update not saved");
    }
}

/// Managed state: the feed and the last result, kept in memory.
pub struct Updater {
    status: Mutex<UpdateStatus>,
    feed: String,
}

impl Updater {
    pub fn new(feed: &str, current: &str) -> Updater {
        Updater {
            status: Mutex::new(UpdateStatus { current: current.to_string(), ..Default::default() }),
            feed: feed.to_string(),
        }
    }

    /// At start: shows an update a check found before the restart, while it is still newer.
    pub fn load_found(&self, store: &Store) {
        let mut s = self.status.lock().unwrap_or_else(|e| e.into_inner());
        match saved_found(store) {
            Some((latest, url)) if is_newer(&s.current, &latest) => {
                s.latest = Some(latest);
                s.url = Some(safe_release_url(Some(&url)));
            }
            Some(_) => save_found(store, None),
            None => {}
        }
    }

    pub fn status(&self) -> UpdateStatus {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Checks now, whatever the switch and the clock say, and saves the time.
    pub async fn check_now(&self, client: &reqwest::Client, store: &Store) -> UpdateStatus {
        let current = self.status().current;
        let mut s = check(client, &self.feed, &current).await;
        save_last_check(store, Utc::now());
        let mut held = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if s.error.is_some() {
            // A failed check says nothing new: keep an update found earlier.
            s.latest = held.latest.clone();
            s.url = held.url.clone();
        } else {
            save_found(store, s.latest.as_deref().zip(s.url.as_deref()));
        }
        *held = s.clone();
        s
    }

    /// One turn of the hourly loop: checks only when `on` and a check is due. `Some` when it checked.
    pub async fn tick(
        &self,
        client: &reqwest::Client,
        store: &Store,
        on: bool,
        now: DateTime<Utc>,
    ) -> Option<UpdateStatus> {
        if !on || !due(last_check(store).as_deref(), now) {
            return None;
        }
        Some(self.check_now(client, store).await)
    }
}

/// The event payload for `haul:update-available`, when `s` reports a newer release.
pub fn available(s: &UpdateStatus) -> Option<serde_json::Value> {
    match (&s.latest, &s.url) {
        (Some(v), Some(u)) => Some(serde_json::json!({ "version": v, "url": u })),
        _ => None,
    }
}

/// 30 s after start, then once an hour: checks when the switch is on and the last check is a day old.
pub fn spawn_loop(app: tauri::AppHandle, core: crate::core::AppCore) {
    use tauri::{Emitter, Manager};
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let client = client();
        let mut hourly = tokio::time::interval(Duration::from_secs(3600));
        loop {
            hourly.tick().await;
            let on = core.settings().check_updates;
            let updater = app.state::<Updater>();
            if let Some(s) = updater.tick(&client, &core.store, on, Utc::now()).await
                && let Some(ev) = available(&s)
            {
                tracing::info!(latest = ?s.latest, "a newer Haul release exists");
                let _ = app.emit("haul:update-available", ev);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use chrono::Duration as Days;
    use std::sync::{Arc, Mutex as StdMutex};

    const TAG_URL: &str = "https://github.com/meraaki410/haul/releases/tag/v0.2.0";

    #[test]
    fn newer_versions_table() {
        assert!(is_newer("0.1.0", "v0.2.0"));
        assert!(is_newer("0.9.9", "0.10.0"));
        assert!(is_newer("0.1.0", "1.0.0"));
        assert!(!is_newer("0.1.0", "v0.1.0"));
        assert!(!is_newer("0.2.0", "0.1.9"));
        assert!(!is_newer("0.1.0", "v1.0.0-beta"));
        assert!(!is_newer("0.1.0", "latest"));
        assert!(!is_newer("0.1.0", ""));
    }

    #[test]
    fn foreign_url_falls_back() {
        assert_eq!(safe_release_url(Some("https://evil.example/x")), "https://github.com/meraaki410/haul/releases");
        assert_eq!(safe_release_url(Some(TAG_URL)), TAG_URL);
        assert_eq!(safe_release_url(None), "https://github.com/meraaki410/haul/releases");
        // A look-alike repository name is not ours.
        assert_eq!(
            safe_release_url(Some("https://github.com/meraaki410/haul-evil/releases/x")),
            "https://github.com/meraaki410/haul/releases"
        );
    }

    #[test]
    fn due_after_a_day() {
        let now = Utc::now();
        assert!(due(None, now));
        assert!(due(Some("junk"), now));
        assert!(!due(Some(&(now - Days::hours(23)).to_rfc3339()), now));
        assert!(due(Some(&(now - Days::hours(24)).to_rfc3339()), now));
    }

    /// A local `/latest` that answers with `status` and `body` and records each request's headers.
    async fn feed(status: StatusCode, body: &'static str) -> (String, Arc<StdMutex<Vec<HeaderMap>>>) {
        let seen: Arc<StdMutex<Vec<HeaderMap>>> = Arc::default();
        let s = seen.clone();
        let app = axum::Router::new().route(
            "/latest",
            axum::routing::get(move |h: HeaderMap| {
                let s = s.clone();
                async move {
                    s.lock().unwrap().push(h);
                    (status, body).into_response()
                }
            }),
        );
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/latest", l.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        (url, seen)
    }

    const NEWER: &str = r#"{"tag_name":"v0.2.0","html_url":"https://github.com/meraaki410/haul/releases/tag/v0.2.0"}"#;

    #[tokio::test]
    async fn newer_release_reported() {
        let (url, _) = feed(StatusCode::OK, NEWER).await;
        let s = check(&client(), &url, "0.1.0").await;
        assert_eq!(s.latest.as_deref(), Some("0.2.0"));
        assert_eq!(s.url.as_deref(), Some(TAG_URL));
        assert_eq!(s.current, "0.1.0");
        assert!(s.error.is_none() && s.checked_at.is_some());
    }

    #[tokio::test]
    async fn same_release_is_up_to_date() {
        let (url, _) = feed(StatusCode::OK, r#"{"tag_name":"v0.1.0","html_url":"x"}"#).await;
        let s = check(&client(), &url, "0.1.0").await;
        assert!(s.latest.is_none() && s.url.is_none() && s.error.is_none(), "{s:?}");
    }

    #[tokio::test]
    async fn no_release_404_is_quiet() {
        let (url, _) = feed(StatusCode::NOT_FOUND, r#"{"message":"Not Found"}"#).await;
        let s = check(&client(), &url, "0.1.0").await;
        assert!(s.latest.is_none() && s.error.is_none(), "{s:?}");
    }

    #[tokio::test]
    async fn rate_limited_reply_is_an_error_not_a_crash() {
        let (url, _) = feed(StatusCode::FORBIDDEN, r#"{"message":"API rate limit exceeded"}"#).await;
        let s = check(&client(), &url, "0.1.0").await;
        assert!(s.latest.is_none());
        assert!(s.error.as_deref().is_some_and(|e| e.contains("403")), "{s:?}");
    }

    #[tokio::test]
    async fn server_error_is_an_error() {
        let (url, _) = feed(StatusCode::INTERNAL_SERVER_ERROR, "oops").await;
        let s = check(&client(), &url, "0.1.0").await;
        assert!(s.error.as_deref().is_some_and(|e| e.contains("500")), "{s:?}");
    }

    #[tokio::test]
    async fn bad_json_is_an_error() {
        let (url, _) = feed(StatusCode::OK, "not json").await;
        let s = check(&client(), &url, "0.1.0").await;
        assert!(s.latest.is_none());
        assert_eq!(s.error.as_deref(), Some("couldn't read GitHub's reply"));
    }

    #[tokio::test]
    async fn unreachable_feed_is_an_error() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/latest", l.local_addr().unwrap());
        drop(l);
        let s = check(&client(), &url, "0.1.0").await;
        assert!(s.error.is_some(), "{s:?}");
    }

    #[tokio::test]
    async fn request_sends_user_agent() {
        let (url, seen) = feed(StatusCode::OK, NEWER).await;
        check(&client(), &url, "0.1.0").await;
        let h = seen.lock().unwrap()[0].clone();
        assert_eq!(h["user-agent"], format!("Haul/{}", env!("CARGO_PKG_VERSION")).as_str());
        assert_eq!(h["accept"], "application/vnd.github+json");
    }

    #[tokio::test]
    async fn off_switch_sends_nothing() {
        let (url, seen) = feed(StatusCode::OK, NEWER).await;
        let store = Store::open_in_memory().unwrap();
        let u = Updater::new(&url, "0.1.0");
        assert!(u.tick(&client(), &store, false, Utc::now()).await.is_none());
        assert!(seen.lock().unwrap().is_empty());
        assert!(last_check(&store).is_none());
    }

    #[tokio::test]
    async fn recent_check_is_skipped() {
        let (url, seen) = feed(StatusCode::OK, NEWER).await;
        let store = Store::open_in_memory().unwrap();
        let now = Utc::now();
        save_last_check(&store, now - Days::hours(2));
        let u = Updater::new(&url, "0.1.0");
        assert!(u.tick(&client(), &store, true, now).await.is_none());
        assert!(seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn due_check_runs_saves_time_and_keeps_status() {
        let (url, seen) = feed(StatusCode::OK, NEWER).await;
        let store = Store::open_in_memory().unwrap();
        let u = Updater::new(&url, "0.1.0");
        assert_eq!(u.status().current, "0.1.0");
        let s = u.tick(&client(), &store, true, Utc::now()).await.expect("checked");
        assert_eq!(s.latest.as_deref(), Some("0.2.0"));
        assert_eq!(seen.lock().unwrap().len(), 1);
        assert!(last_check(&store).is_some());
        assert_eq!(u.status(), s);
    }

    #[tokio::test]
    async fn failed_check_still_saves_time() {
        let (url, _) = feed(StatusCode::FORBIDDEN, "{}").await;
        let store = Store::open_in_memory().unwrap();
        let u = Updater::new(&url, "0.1.0");
        let s = u.check_now(&client(), &store).await;
        assert!(s.error.is_some());
        assert!(last_check(&store).is_some());
    }

    #[tokio::test]
    async fn failed_check_keeps_found_update() {
        let (good, _) = feed(StatusCode::OK, NEWER).await;
        let (bad, _) = feed(StatusCode::INTERNAL_SERVER_ERROR, "oops").await;
        let store = Store::open_in_memory().unwrap();
        let mut u = Updater::new(&good, "0.1.0");
        u.check_now(&client(), &store).await;
        u.feed = bad;
        let s = u.check_now(&client(), &store).await;
        assert_eq!(s.latest.as_deref(), Some("0.2.0"));
        assert_eq!(s.url.as_deref(), Some(TAG_URL));
        assert!(s.error.is_some(), "{s:?}");
    }

    #[tokio::test]
    async fn found_update_survives_restart() {
        let (url, _) = feed(StatusCode::OK, NEWER).await;
        let store = Store::open_in_memory().unwrap();
        Updater::new(&url, "0.1.0").check_now(&client(), &store).await;
        let after_restart = Updater::new(&url, "0.1.0");
        after_restart.load_found(&store);
        assert_eq!(after_restart.status().latest.as_deref(), Some("0.2.0"));
        assert_eq!(after_restart.status().url.as_deref(), Some(TAG_URL));
    }

    #[tokio::test]
    async fn saved_update_cleared_once_installed() {
        let (url, _) = feed(StatusCode::OK, NEWER).await;
        let store = Store::open_in_memory().unwrap();
        Updater::new(&url, "0.1.0").check_now(&client(), &store).await;
        let upgraded = Updater::new(&url, "0.2.0");
        upgraded.load_found(&store);
        assert_eq!(upgraded.status().latest, None);
        assert_eq!(saved_found(&store), None);
    }

    #[tokio::test]
    async fn up_to_date_check_clears_saved_update() {
        let (newer, _) = feed(StatusCode::OK, NEWER).await;
        let (same, _) = feed(StatusCode::OK, r#"{"tag_name":"v0.1.0","html_url":"x"}"#).await;
        let store = Store::open_in_memory().unwrap();
        let mut u = Updater::new(&newer, "0.1.0");
        u.check_now(&client(), &store).await;
        u.feed = same;
        assert_eq!(u.check_now(&client(), &store).await.latest, None);
        assert_eq!(saved_found(&store), None);
    }

    #[test]
    fn release_url_cannot_climb() {
        for bad in [
            "https://github.com/meraaki410/haul/releases/../../evil",
            "https://github.com/meraaki410/haul/releases/%2e%2e/%2e%2e/evil",
            "https://github.com.evil.io/meraaki410/haul/releases/x",
            "https://github.com@evil.io/meraaki410/haul/releases/x",
            "http://github.com/meraaki410/haul/releases/x",
        ] {
            assert_eq!(safe_release_url(Some(bad)), RELEASES, "{bad}");
        }
        assert_eq!(safe_release_url(Some(TAG_URL)), TAG_URL);
    }

    #[test]
    fn status_serialises_camel_case() {
        let s = UpdateStatus { current: "0.1.0".into(), checked_at: Some("t".into()), ..Default::default() };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v, serde_json::json!({"current":"0.1.0","latest":null,"url":null,"checkedAt":"t","error":null}));
    }
}
