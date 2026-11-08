//! Secrets never reach the log. Its own test binary, because it installs a global log catcher.

use chrono::NaiveTime;
use haul_app_lib::api::pairing::PairGate;
use haul_app_lib::api::server::start_api;
use haul_app_lib::core::AppCore;
use haul_engine::{Schedule, Store};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
struct Buf(Arc<Mutex<Vec<u8>>>);

impl Write for Buf {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn capture_log_has_no_secrets() {
    let buf = Buf::default();
    let w = buf.clone();
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_ansi(false)
        .with_writer(move || w.clone())
        .try_init()
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let t = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let never = Some(Schedule { start: t, stop: t, weekdays: vec![] });
    let core = AppCore::start_with(Store::open_in_memory().unwrap(), dir.path().to_path_buf(), never);
    let gate = PairGate::new(Duration::from_secs(5));
    let g = gate.clone();
    let p = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let api = start_api(core, gate, Arc::new(move |_| g.answer(true)), p..=p + 2, None).await.unwrap();
    let http = haul_engine::http_client();
    let url = |path: &str| format!("http://127.0.0.1:{}{path}", api.port);
    let post = |path: &str, body: serde_json::Value| {
        http.post(url(path))
            .header("origin", "chrome-extension://testid")
            .header("content-type", "application/json")
            .body(body.to_string())
    };

    let r = post("/v1/pair", serde_json::json!({"browser": "Microsoft Edge"})).send().await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&r.text().await.unwrap()).unwrap();
    let token = v["token"].as_str().unwrap().to_string();
    let body = serde_json::json!({
        "url": "https://u:hunter2@a.org/f.zip",
        "cookies": "sid=topsecret; other=alsosecret",
        "referrer": "https://a.org/private-page?key=refsecret",
    });
    let r = post("/v1/capture", body).bearer_auth(&token).send().await.unwrap();
    assert_eq!(r.status(), 200);

    let log = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
    assert!(log.contains("a.org"), "the capture is logged: {log}");
    assert!(log.contains("cookies=2"), "{log}");
    for secret in ["topsecret", "alsosecret", "hunter2", "refsecret", token.as_str()] {
        assert!(!log.contains(secret), "{secret} leaked into the log: {log}");
    }
}
