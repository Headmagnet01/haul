//! Downloads and links that arrive from the browser extension.

use crate::api::guard::{hash_token, new_token};
use crate::core::AppCore;
use crate::dto::{CaptureRequest, IntakeResult, PairedClientView};
use haul_engine::{DownloadState, EngineError, NewDownload, Priority};
use std::time::{Duration, Instant};

/// The same URL captured again within this window is the same download (a double click).
const SAME_CAPTURE: Duration = Duration::from_secs(5);

fn invalid(msg: &str) -> EngineError {
    EngineError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, msg.to_string()))
}

/// Only the headers that were sent, under their HTTP names.
fn headers_json(r: &CaptureRequest) -> Option<String> {
    let mut m = serde_json::Map::new();
    for (k, v) in [("Cookie", &r.cookies), ("Referer", &r.referrer), ("User-Agent", &r.user_agent)] {
        if let Some(v) = v.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            m.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }
    }
    (!m.is_empty()).then(|| serde_json::Value::Object(m).to_string())
}

impl AppCore {
    /// Queues one download from the browser and starts it. Returns its id.
    pub fn capture(&self, req: CaptureRequest) -> Result<i64, EngineError> {
        let url = url::Url::parse(&req.url).map_err(|_| invalid("not a link"))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(invalid("only http and https links can be captured"));
        }
        let host = url.host_str().unwrap_or_default().to_string();
        let cookies = req.cookies.as_deref().map_or(0, |c| c.split(';').filter(|p| !p.trim().is_empty()).count());
        let headers = headers_json(&req);

        if self.settings().browser_start == crate::settings::BrowserStart::Collector {
            // The link keeps the capture's headers, sealed, so a login-only link checks and starts.
            self.grabber.intake_capture(req.url.clone(), headers)?;
            self.note_first_download();
            tracing::info!(host = %host, cookies, "captured to the Collector");
            return Ok(0);
        }
        let mut recent = self.recent.lock().unwrap_or_else(|e| e.into_inner());
        recent.retain(|_, (_, at)| at.elapsed() < SAME_CAPTURE);
        if let Some((id, _)) = recent.get(&req.url) {
            return Ok(*id);
        }
        let revive = self
            .store
            .find_by_url(&req.url)?
            .into_iter()
            .find(|d| matches!(d.state, DownloadState::LinkExpired | DownloadState::Paused));
        let id = match revive {
            Some(d) => {
                self.store.refresh_headers(d.id, headers.as_deref())?;
                self.queue.resume(d.id);
                tracing::info!(host = %host, id = d.id, cookies, "browser capture refreshed a download");
                d.id
            }
            None => {
                let name = req.filename.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
                let id = self.queue.add(NewDownload {
                    package_id: None,
                    url: req.url.clone(),
                    headers_json: headers,
                    target_dir: self.download_dir(),
                    file_name: name.clone(),
                    priority: Priority::Normal,
                    connections: self.connections(),
                    speed_limit: None,
                })?;
                tracing::info!(host = %host, file = name.as_deref().unwrap_or(""), cookies, "captured from the browser");
                id
            }
        };
        recent.insert(req.url, (id, Instant::now()));
        drop(recent);
        self.note_first_download();
        Ok(id)
    }

    /// Sends links picked on a web page to the Collector, as one batch named after the page.
    pub fn add_browser_links(&self, links: Vec<String>, page_title: Option<&str>) -> Result<IntakeResult, EngineError> {
        let links: Vec<String> = links
            .into_iter()
            .filter(|l| url::Url::parse(l).is_ok_and(|u| matches!(u.scheme(), "http" | "https")))
            .collect();
        if links.is_empty() {
            return Ok(IntakeResult { added: 0, skipped: 0 });
        }
        let r = self.grabber.intake_labelled(links, "browser", page_title)?;
        self.note_first_download();
        Ok(r)
    }

    /// Pairs a browser extension: stores a hash of a fresh key bound to its origin, and returns the key.
    pub fn pair_client(&self, browser: &str, origin: &str) -> Result<String, EngineError> {
        let token = new_token();
        self.store.add_paired_client(browser, origin, &hash_token(&token))?;
        tracing::info!(browser, "browser paired");
        Ok(token)
    }

    /// The paired client this key belongs to, if the key is known and used from the origin it was given to.
    pub fn client_for(&self, token: &str, origin: &str) -> Option<i64> {
        let c = self.store.paired_client_by_hash(&hash_token(token)).ok().flatten()?;
        if c.origin != origin {
            return None;
        }
        if let Err(e) = self.store.touch_paired_client(c.id) {
            tracing::warn!(error = %e, "could not record when a browser was last seen");
        }
        Some(c.id)
    }

    pub fn paired_clients(&self) -> Result<Vec<PairedClientView>, EngineError> {
        Ok(self
            .store
            .list_paired_clients()?
            .into_iter()
            .map(|c| PairedClientView {
                id: c.id,
                browser: c.browser,
                created_at: c.created_at,
                last_seen: c.last_seen,
            })
            .collect())
    }

    /// Forgets a paired browser; its key stops working at once.
    pub fn remove_paired_client(&self, id: i64) -> Result<(), EngineError> {
        self.store.remove_paired_client(id)
    }

    /// Downloads queued or running.
    pub fn active_count(&self) -> usize {
        self.store
            .list_downloads()
            .map(|ds| ds.iter().filter(|d| matches!(d.state, DownloadState::Queued | DownloadState::Running)).count())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use crate::core::AppCore;
    use crate::dto::{CaptureRequest, IntakeResult};
    use chrono::NaiveTime;
    use haul_engine::{DownloadState, Schedule, Store};
    use std::time::{Duration, Instant};

    fn core(dir: &std::path::Path) -> AppCore {
        let t = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        AppCore::start_with(
            Store::open_in_memory().unwrap(),
            dir.to_path_buf(),
            Some(Schedule { start: t, stop: t, weekdays: vec![] }),
        )
    }

    fn req(url: &str) -> CaptureRequest {
        CaptureRequest { url: url.into(), filename: None, size: None, referrer: None, user_agent: None, cookies: None }
    }

    #[tokio::test]
    async fn capture_queues_with_encrypted_headers() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(dir.path());
        let id = c
            .capture(CaptureRequest {
                filename: Some("f.zip".into()),
                size: Some(5_000_000),
                referrer: Some("https://a.org/page".into()),
                user_agent: Some("Mozilla/5.0 Test".into()),
                cookies: Some("sid=1; theme=dark".into()),
                ..req("https://a.org/f.zip")
            })
            .unwrap();
        let d = c.store.get_download(id).unwrap();
        let h: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(d.headers_json.as_deref().unwrap()).unwrap();
        let mut keys: Vec<&str> = h.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, vec!["Cookie", "Referer", "User-Agent"]);
        assert_eq!(h["Cookie"], "sid=1; theme=dark");
        assert_eq!(d.file_name.as_deref(), Some("f.zip"));
        assert_eq!(d.state, DownloadState::Queued);
        assert_eq!(d.package_id, None);
    }

    #[tokio::test]
    async fn capture_without_extras_has_no_headers() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(dir.path());
        let id = c.capture(req("https://a.org/f.zip")).unwrap();
        assert_eq!(c.store.get_download(id).unwrap().headers_json, None);
    }

    #[tokio::test]
    async fn capture_twice_within_5s_returns_same_id() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(dir.path());
        let a = c.capture(req("https://a.org/f.zip")).unwrap();
        let b = c.capture(req("https://a.org/f.zip")).unwrap();
        assert_eq!(a, b);
        assert_eq!(c.store.list_downloads().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn capture_after_5s_adds_new() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(dir.path());
        let a = c.capture(req("https://a.org/f.zip")).unwrap();
        c.recent.lock().unwrap().insert("https://a.org/f.zip".into(), (a, Instant::now() - Duration::from_secs(6)));
        let b = c.capture(req("https://a.org/f.zip")).unwrap();
        assert_ne!(a, b);
        assert_eq!(c.store.list_downloads().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn recapture_refreshes_expired_download() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(dir.path());
        let id = c.capture(CaptureRequest { cookies: Some("sid=old".into()), ..req("https://a.org/f.zip") }).unwrap();
        c.store.set_state(id, DownloadState::LinkExpired, Some("link expired (401)")).unwrap();
        c.recent.lock().unwrap().clear();
        let again =
            c.capture(CaptureRequest { cookies: Some("sid=new".into()), ..req("https://a.org/f.zip") }).unwrap();
        assert_eq!(again, id);
        let d = c.store.get_download(id).unwrap();
        assert!(d.headers_json.unwrap().contains("sid=new"));
        assert_eq!(d.state, DownloadState::Queued);
        assert_eq!(c.store.list_downloads().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn browser_links_go_to_collector_with_label() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(dir.path());
        let r = c
            .add_browser_links(vec!["https://a.org/1.bin".into(), "https://a.org/2.bin".into()], Some("My page"))
            .unwrap();
        assert_eq!(r, IntakeResult { added: 2, skipped: 0 });
        let batches = c.store.list_batches().unwrap();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].label, "My page");
        assert_eq!(batches[0].source, "browser");
    }

    #[tokio::test]
    async fn add_links_accepts_browser_source() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(dir.path());
        assert_eq!(c.add_links("https://a.org/x.bin", "browser").unwrap().added, 1);
    }

    #[tokio::test]
    async fn capture_rejects_non_http_url() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(dir.path());
        assert!(c.capture(req("ftp://a.org/f.zip")).is_err());
        assert!(c.capture(req("blob:https://a.org/1234")).is_err());
        assert!(c.store.list_downloads().unwrap().is_empty());
    }

    fn to_collector(c: &AppCore) {
        let patch = crate::settings::SettingsPatch {
            browser_start: Some(crate::settings::BrowserStart::Collector),
            ..Default::default()
        };
        c.set_settings(patch).unwrap();
    }

    #[tokio::test]
    async fn collector_capture_keeps_cookies() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(dir.path());
        to_collector(&c);
        let id = c
            .capture(CaptureRequest {
                cookies: Some("sid=1".into()),
                referrer: Some("https://a.org/page".into()),
                ..req("https://a.org/f.zip")
            })
            .unwrap();
        assert_eq!(id, 0);
        let links = c.store.list_links().unwrap();
        assert_eq!(links.len(), 1);
        let h = c.store.link_headers(links[0].id).unwrap().expect("headers kept");
        assert!(h.contains("sid=1") && h.contains("Referer"), "{h}");
    }

    const COOKIE_FILE_LEN: usize = 300_000;

    /// A file that only downloads with the cookie `sid=1`; anything else gets 401.
    async fn cookie_only_server() -> String {
        use axum::http::{HeaderMap, StatusCode, header};
        use axum::response::IntoResponse;
        async fn file(headers: HeaderMap) -> axum::response::Response {
            let ok = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()).is_some_and(|c| c.contains("sid=1"));
            if !ok {
                return StatusCode::UNAUTHORIZED.into_response();
            }
            ([(header::CONTENT_TYPE, "application/octet-stream")], vec![7u8; COOKIE_FILE_LEN]).into_response()
        }
        let app = axum::Router::new().route("/f.bin", axum::routing::get(file));
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        format!("http://{addr}/f.bin")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn collector_capture_starts_with_cookies() {
        let url = cookie_only_server().await;
        let dir = tempfile::tempdir().unwrap();
        let c = AppCore::start_with(Store::open_in_memory().unwrap(), dir.path().to_path_buf(), None);
        to_collector(&c);
        c.capture(CaptureRequest { cookies: Some("sid=1".into()), ..req(&url) }).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let link = loop {
            let s = c.collector_snapshot().unwrap();
            if let Some(l) = s.links.iter().find(|l| l.status != haul_engine::LinkStatus::Checking) {
                break l.clone();
            }
            assert!(Instant::now() < deadline, "link never checked");
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert_eq!(link.status, haul_engine::LinkStatus::Online, "{link:?}");
        assert_eq!(c.collector_start(&[link.id]).unwrap().started, 1);
        let deadline = Instant::now() + Duration::from_secs(30);
        let d = loop {
            let d = c.store.list_downloads().unwrap().remove(0);
            if matches!(d.state, DownloadState::Completed | DownloadState::Failed | DownloadState::LinkExpired) {
                break d;
            }
            assert!(Instant::now() < deadline, "download never finished: {d:?}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert_eq!(d.state, DownloadState::Completed, "{d:?}");
        // Completing starts a post-processing run whose sort step moves the file and saves the
        // new path, so the row read above may hold a path that is already stale.
        let deadline = Instant::now() + Duration::from_secs(15);
        let run = loop {
            let runs = c.store.list_runs().unwrap();
            if let Some(r) = runs.into_iter().find(|r| r.download_id == Some(d.id))
                && matches!(r.state.as_str(), "done" | "failed" | "skipped")
            {
                break r;
            }
            assert!(Instant::now() < deadline, "post-processing never finished");
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert_eq!(run.state, "done", "{run:?}");
        let path = c.store.get_download(d.id).unwrap().target_path.expect("saved");
        assert_eq!(std::fs::metadata(path).unwrap().len(), COOKIE_FILE_LEN as u64);
    }

    #[tokio::test]
    async fn active_count_counts_queued_and_running() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(dir.path());
        assert_eq!(c.active_count(), 0);
        let id = c.capture(req("https://a.org/f.zip")).unwrap();
        c.capture(req("https://a.org/g.zip")).unwrap();
        assert_eq!(c.active_count(), 2);
        c.store.set_state(id, DownloadState::Paused, None).unwrap();
        assert_eq!(c.active_count(), 1);
    }
}
