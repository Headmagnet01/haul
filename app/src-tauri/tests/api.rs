//! The local API, end to end over real HTTP on 127.0.0.1.

use chrono::NaiveTime;
use haul_app_lib::api::pairing::PairGate;
use haul_app_lib::api::server::{ApiHandle, PairEvent, start_api};
use haul_app_lib::core::AppCore;
use haul_engine::{Schedule, Store};
use std::sync::Arc;
use std::time::Duration;

const EXT: &str = "chrome-extension://testid";

struct Env {
    core: AppCore,
    api: ApiHandle,
    http: reqwest::Client,
    _dir: tempfile::TempDir,
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn never() -> Option<Schedule> {
    let t = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    Some(Schedule { start: t, stop: t, weekdays: vec![] })
}

/// `answer`: what the "window" says to each pairing request (None = it never answers).
async fn env_with(answer: Option<bool>, timeout: Duration) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let core = AppCore::start_with(Store::open_in_memory().unwrap(), dir.path().to_path_buf(), never());
    let gate = PairGate::new(timeout);
    let g = gate.clone();
    let on_pair: Arc<dyn Fn(PairEvent) + Send + Sync> = Arc::new(move |ev| {
        if let (PairEvent::Asked { n, .. }, Some(a)) = (ev, answer) {
            g.answer(n, a);
        }
    });
    let p = free_port();
    let api = start_api(core.clone(), gate, on_pair, p..=p + 2, None).await.unwrap();
    Env { core, api, http: haul_engine::http_client(), _dir: dir }
}

async fn env() -> Env {
    env_with(Some(true), Duration::from_secs(5)).await
}

impl Env {
    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.api.port)
    }

    // reqwest adds a repeated header rather than replacing it, so each request names its origin once.
    fn get_from(&self, path: &str, origin: &str) -> reqwest::RequestBuilder {
        self.http.get(self.url(path)).header("origin", origin)
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.get_from(path, EXT)
    }

    /// Status is a POST: Chrome sends no Origin on an extension's GET to a host it may access.
    fn status_from(&self, origin: &str) -> reqwest::RequestBuilder {
        self.http.post(self.url("/v1/status")).header("origin", origin)
    }

    fn status(&self) -> reqwest::RequestBuilder {
        self.status_from(EXT)
    }

    fn post_from(&self, path: &str, body: serde_json::Value, origin: &str) -> reqwest::RequestBuilder {
        self.http
            .post(self.url(path))
            .header("origin", origin)
            .header("content-type", "application/json")
            .body(body.to_string())
    }

    fn post(&self, path: &str, body: serde_json::Value) -> reqwest::RequestBuilder {
        self.post_from(path, body, EXT)
    }

    async fn pair_from(&self, origin: &str) -> String {
        let r =
            self.post_from("/v1/pair", serde_json::json!({"browser": "Microsoft Edge"}), origin).send().await.unwrap();
        assert_eq!(r.status(), 200);
        let v: serde_json::Value = serde_json::from_str(&r.text().await.unwrap()).unwrap();
        v["token"].as_str().unwrap().to_string()
    }

    async fn pair(&self) -> String {
        self.pair_from(EXT).await
    }
}

async fn json(r: reqwest::Response) -> serde_json::Value {
    serde_json::from_str(&r.text().await.unwrap()).unwrap()
}

fn capture_body() -> serde_json::Value {
    serde_json::json!({"url": "https://a.org/f.zip", "filename": "f.zip", "cookies": "sid=1"})
}

#[tokio::test]
async fn status_needs_no_token() {
    let e = env().await;
    let r = e.status().send().await.unwrap();
    assert_eq!(r.status(), 200);
    let v = json(r).await;
    assert_eq!(v["app"], "haul");
    assert_eq!(v["paired"], false);
    assert_eq!(v["active"], 0);
    assert!(v["version"].as_str().is_some_and(|s| !s.is_empty()));
}

#[tokio::test]
async fn status_by_get_is_refused() {
    let e = env().await;
    let r = e.get("/v1/status").send().await.unwrap();
    assert_eq!(r.status(), 405);
}

#[tokio::test]
async fn wrong_host_is_421() {
    let e = env().await;
    let r = e.status().header("host", format!("evil.test:{}", e.api.port)).send().await.unwrap();
    assert_eq!(r.status(), 421);
}

#[tokio::test]
async fn web_origin_is_403() {
    let e = env().await;
    let r = e.status_from("https://evil.test").send().await.unwrap();
    assert_eq!(r.status(), 403);
}

#[tokio::test]
async fn missing_origin_is_403() {
    let e = env().await;
    let r = e.http.post(e.url("/v1/status")).send().await.unwrap();
    assert_eq!(r.status(), 403);
}

#[tokio::test]
async fn capture_without_token_is_401() {
    let e = env().await;
    let r = e.post("/v1/capture", capture_body()).send().await.unwrap();
    assert_eq!(r.status(), 401);
    assert!(e.core.snapshot().unwrap().downloads.is_empty());
}

#[tokio::test]
async fn pair_allow_returns_token_then_capture_works() {
    let e = env().await;
    let token = e.pair().await;
    assert_eq!(token.len(), 64);
    let r = e.post("/v1/capture", capture_body()).bearer_auth(&token).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert!(json(r).await["id"].as_i64().is_some());
    let r = e.status().bearer_auth(&token).send().await.unwrap();
    let v = json(r).await;
    assert_eq!(v["paired"], true);
    assert_eq!(v["active"], 1);
    let clients = e.core.paired_clients().unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].browser, "Microsoft Edge");
    assert!(clients[0].last_seen.is_some());
}

#[tokio::test]
async fn pair_deny_is_403() {
    let e = env_with(Some(false), Duration::from_secs(5)).await;
    let r = e.post("/v1/pair", serde_json::json!({"browser": "Google Chrome"})).send().await.unwrap();
    assert_eq!(r.status(), 403);
    assert!(e.core.paired_clients().unwrap().is_empty());
}

#[tokio::test]
async fn second_pair_while_waiting_is_429() {
    let e = env_with(None, Duration::from_secs(3)).await;
    let first = e.post("/v1/pair", serde_json::json!({"browser": "A"})).send();
    let first = tokio::spawn(first);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let r = e.post("/v1/pair", serde_json::json!({"browser": "B"})).send().await.unwrap();
    assert_eq!(r.status(), 429);
    assert_eq!(first.await.unwrap().unwrap().status(), 403, "the unanswered first request times out as Deny");
}

#[tokio::test]
async fn token_from_other_extension_is_401() {
    let e = env().await;
    let token = e.pair_from("chrome-extension://aaaa").await;
    let r =
        e.post_from("/v1/capture", capture_body(), "chrome-extension://bbbb").bearer_auth(&token).send().await.unwrap();
    assert_eq!(r.status(), 401);
}

#[tokio::test]
async fn firefox_pairs_and_captures() {
    let e = env().await;
    let ff = "moz-extension://6c1d9f2e-1b7a-4e3c-9a55-0d2f7e8b4c31";
    let token = e.pair_from(ff).await;
    let r = e.post_from("/v1/capture", capture_body(), ff).bearer_auth(&token).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let r = e.post_from("/v1/capture", capture_body(), EXT).bearer_auth(&token).send().await.unwrap();
    assert_eq!(r.status(), 401, "a Firefox key is bound to its own origin");
}

#[tokio::test]
async fn removed_client_token_is_401() {
    let e = env().await;
    let token = e.pair().await;
    let id = e.core.paired_clients().unwrap()[0].id;
    e.core.remove_paired_client(id).unwrap();
    let r = e.post("/v1/capture", capture_body()).bearer_auth(&token).send().await.unwrap();
    assert_eq!(r.status(), 401);
}

#[tokio::test]
async fn links_land_in_collector() {
    let e = env().await;
    let token = e.pair().await;
    let body = serde_json::json!({"links": ["https://a.org/1.bin", "https://a.org/2.bin"], "page_title": "A page"});
    let r = e.post("/v1/links", body).bearer_auth(&token).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let v = json(r).await;
    assert_eq!(v["added"], 2);
    assert_eq!(v["skipped"], 0);
}

#[tokio::test]
async fn too_many_links_is_413() {
    let e = env().await;
    let token = e.pair().await;
    let links: Vec<String> = (0..5001).map(|i| format!("https://a.org/{i}.bin")).collect();
    let r = e.post("/v1/links", serde_json::json!({ "links": links })).bearer_auth(&token).send().await.unwrap();
    assert_eq!(r.status(), 413);
}

#[tokio::test]
async fn body_over_2mib_is_413() {
    let e = env().await;
    let token = e.pair().await;
    let big = "x".repeat(2 * 1024 * 1024 + 1);
    let r = e
        .post("/v1/capture", serde_json::json!({"url": "https://a.org/f.zip", "cookies": big}))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 413);
}

#[tokio::test]
async fn bad_capture_is_400() {
    let e = env().await;
    let token = e.pair().await;
    let r =
        e.post("/v1/capture", serde_json::json!({"url": "ftp://a.org/f"})).bearer_auth(&token).send().await.unwrap();
    assert_eq!(r.status(), 400);
}

#[tokio::test]
async fn port_falls_back_when_taken() {
    let dir = tempfile::tempdir().unwrap();
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let p = held.local_addr().unwrap().port();
    let core = AppCore::start_with(Store::open_in_memory().unwrap(), dir.path().to_path_buf(), never());
    let file = dir.path().join("api-port");
    // A wide range: the other tests bind nearby ports at the same time.
    let api = start_api(core, PairGate::new(Duration::from_secs(1)), Arc::new(|_| {}), p..=p + 50, Some(file.clone()))
        .await
        .unwrap();
    assert_ne!(api.port, p);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), api.port.to_string());
}

/// Quitting removes the port file, so nothing points at a port Haul no longer holds.
#[tokio::test]
async fn stop_removes_port_file() {
    let dir = tempfile::tempdir().unwrap();
    let p = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let core = AppCore::start_with(Store::open_in_memory().unwrap(), dir.path().to_path_buf(), never());
    let file = dir.path().join("api-port");
    let api = start_api(core, PairGate::new(Duration::from_secs(1)), Arc::new(|_| {}), p..=p + 50, Some(file.clone()))
        .await
        .unwrap();
    assert!(file.is_file());
    api.stop();
    assert!(!file.exists(), "api-port is still there after stop");
    // A second stop (or a file someone removed) is fine.
    api.stop();
}

#[tokio::test]
async fn all_ports_taken_is_error() {
    let dir = tempfile::tempdir().unwrap();
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let p = held.local_addr().unwrap().port();
    let core = AppCore::start_with(Store::open_in_memory().unwrap(), dir.path().to_path_buf(), never());
    assert!(start_api(core, PairGate::new(Duration::from_secs(1)), Arc::new(|_| {}), p..=p, None).await.is_err());
}

#[tokio::test]
async fn no_cors_headers() {
    let e = env().await;
    let token = e.pair().await;
    for r in [
        e.status().send().await.unwrap(),
        e.status_from("https://evil.test").send().await.unwrap(),
        e.post("/v1/capture", capture_body()).bearer_auth(&token).send().await.unwrap(),
    ] {
        assert!(r.headers().get("access-control-allow-origin").is_none());
    }
}

#[tokio::test]
async fn stopped_server_answers_503_or_refuses() {
    let e = env().await;
    e.api.stop();
    tokio::time::sleep(Duration::from_millis(100)).await;
    // A refused connection is also fine.
    if let Ok(r) = e.status().send().await {
        assert_eq!(r.status(), 503);
    }
}

#[tokio::test]
async fn pair_request_ends_with_closed_event() {
    let dir = tempfile::tempdir().unwrap();
    let core = AppCore::start_with(Store::open_in_memory().unwrap(), dir.path().to_path_buf(), never());
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let s = seen.clone();
    let p = free_port();
    let api = start_api(
        core,
        PairGate::new(Duration::from_millis(300)),
        Arc::new(move |ev| s.lock().unwrap().push(ev)),
        p..=p + 2,
        None,
    )
    .await
    .unwrap();
    let r = haul_engine::http_client()
        .post(format!("http://127.0.0.1:{}/v1/pair", api.port))
        .header("origin", EXT)
        .header("content-type", "application/json")
        .body(r#"{"browser":"Google Chrome"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403, "unanswered: times out as Deny");
    let seen = seen.lock().unwrap().clone();
    let n = match &seen[0] {
        PairEvent::Asked { n, browser } => {
            assert_eq!(browser, "Google Chrome");
            *n
        }
        other => panic!("first event should be Asked, got {other:?}"),
    };
    assert_eq!(seen[1..], [PairEvent::Closed { n }], "the window is told to close the question");
}
