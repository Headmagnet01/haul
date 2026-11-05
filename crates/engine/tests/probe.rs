mod support;

use haul_engine::{EngineError, ProbeInfo, http_client, probe};
use reqwest::header::HeaderMap;
use std::sync::atomic::Ordering::SeqCst;
use support::{Spec, serve};

async fn probe_spec(spec: Spec) -> Result<ProbeInfo, EngineError> {
    let srv = serve(spec).await;
    probe(&http_client(), &srv.url, &HeaderMap::new()).await
}

#[tokio::test]
async fn probe_range_server() {
    let mut spec = Spec::new(5_000_000);
    spec.etag = Some("abc".into());
    spec.last_modified = Some("Tue, 01 Jan 2030 00:00:00 GMT".into());
    let p = probe_spec(spec).await.unwrap();
    assert_eq!(p.size, Some(5_000_000));
    assert!(p.resumable);
    assert_eq!(p.etag.as_deref(), Some("\"abc\""));
    assert_eq!(p.last_modified.as_deref(), Some("Tue, 01 Jan 2030 00:00:00 GMT"));
    assert_eq!(p.file_name, "file");
}

#[tokio::test]
async fn probe_no_ranges() {
    let mut spec = Spec::new(1000);
    spec.ranges = false;
    let p = probe_spec(spec).await.unwrap();
    assert!(!p.resumable);
    assert_eq!(p.size, Some(1000));
}

#[tokio::test]
async fn probe_head_refused() {
    let mut spec = Spec::new(70_000);
    spec.head_allowed = false;
    let srv = serve(spec).await;
    let p = probe(&http_client(), &srv.url, &HeaderMap::new()).await.unwrap();
    assert!(p.resumable);
    assert_eq!(p.size, Some(70_000));
    assert!(srv.requests.load(SeqCst) >= 2);
}

#[tokio::test]
async fn probe_redirect() {
    let mut spec = Spec::new(10);
    spec.redirect_from = Some("/old".into());
    let srv = serve(spec).await;
    let p = probe(&http_client(), &format!("{}/old", srv.base), &HeaderMap::new())
        .await
        .unwrap();
    assert!(p.final_url.ends_with("/file"), "{}", p.final_url);
}

fn with_status(code: u16) -> Spec {
    let mut s = Spec::new(10);
    s.status = Some(code);
    s
}

#[tokio::test]
async fn probe_404_offline() {
    assert!(matches!(probe_spec(with_status(404)).await, Err(EngineError::Offline)));
}

#[tokio::test]
async fn probe_403_expired() {
    assert!(matches!(probe_spec(with_status(403)).await, Err(EngineError::LinkExpired(403))));
}

#[tokio::test]
async fn probe_410_expired() {
    assert!(matches!(probe_spec(with_status(410)).await, Err(EngineError::LinkExpired(410))));
}

#[tokio::test]
async fn probe_content_disposition() {
    let mut spec = Spec::new(10);
    spec.content_disposition = Some("attachment; filename=\"x y.iso\"".into());
    assert_eq!(probe_spec(spec).await.unwrap().file_name, "x y.iso");
}

#[tokio::test]
async fn probe_unknown_size() {
    let mut spec = Spec::new(2000);
    spec.no_length = true;
    spec.ranges = false;
    let p = probe_spec(spec).await.unwrap();
    assert_eq!(p.size, None);
    assert!(!p.resumable);
}
