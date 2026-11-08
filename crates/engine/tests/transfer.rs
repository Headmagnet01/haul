mod support;

use haul_engine::{
    DownloadId, EngineError, Event, EventBus, HostLimiter, MIB, NewDownload, Priority, RateLimiter, Store,
    TransferConfig, TransferCtx, TransferJob, TransferOutcome, http_client, run_transfer,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering::SeqCst;
use std::time::{Duration, Instant};
use support::{Spec, assert_file_eq, serve};
use tokio_util::sync::CancellationToken;

fn ctx(hosts: usize, config: TransferConfig) -> TransferCtx {
    TransferCtx {
        client: http_client(),
        store: Store::open_in_memory().unwrap(),
        events: EventBus::new(),
        global_limit: Arc::new(RateLimiter::new(None)),
        hosts: Arc::new(HostLimiter::new(hosts)),
        config,
    }
}

fn add(ctx: &TransferCtx, url: &str, dir: &Path, conns: u32, limit: Option<u64>) -> DownloadId {
    ctx.store
        .insert_download(&NewDownload {
            package_id: None,
            url: url.to_string(),
            headers_json: None,
            target_dir: dir.to_path_buf(),
            file_name: None,
            priority: Priority::Normal,
            connections: conns,
            speed_limit: limit,
        })
        .unwrap()
}

async fn run(ctx: &TransferCtx, id: DownloadId) -> TransferOutcome {
    run_transfer(TransferJob { id, cancel: CancellationToken::new() }, ctx).await
}

fn completed(o: TransferOutcome) -> PathBuf {
    match o {
        TransferOutcome::Completed(p) => p,
        other => panic!("expected Completed, got {other:?}"),
    }
}

fn part_files(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".haulpart"))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn single_small_file() {
    let srv = serve(Spec::new(100_000)).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 8, None);
    let p = completed(run(&c, id).await);
    assert_eq!(p, dir.path().join("file"));
    assert_file_eq(&p, 100_000);
    assert!(part_files(dir.path()).is_empty());
    let d = c.store.get_download(id).unwrap();
    assert_eq!(d.state, haul_engine::DownloadState::Completed);
    assert_eq!(d.target_path, Some(p));
}

#[tokio::test]
async fn multi_connection_used() {
    let len = 32 * MIB as usize;
    let mut spec = Spec::new(len);
    spec.bytes_per_sec = Some(4_000_000);
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 8, None);
    let p = completed(run(&c, id).await);
    assert!(srv.max_concurrent.load(SeqCst) >= 4, "max_concurrent {}", srv.max_concurrent.load(SeqCst));
    assert_file_eq(&p, len);
}

#[tokio::test]
async fn no_range_server_single_connection() {
    let mut spec = Spec::new(3_000_000);
    spec.ranges = false;
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 8, None);
    let p = completed(run(&c, id).await);
    assert_eq!(srv.max_concurrent.load(SeqCst), 1);
    assert_file_eq(&p, 3_000_000);
}

#[tokio::test]
async fn unknown_size_streams_to_eof() {
    let mut spec = Spec::new(2_000_000);
    spec.no_length = true;
    spec.ranges = false;
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 8, None);
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, 2_000_000);
    assert!(part_files(dir.path()).is_empty());
}

#[tokio::test]
async fn name_conflict_never_overwrites() {
    let srv = serve(Spec::new(50_000)).await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file"), b"keep").unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 4, None);
    let p = completed(run(&c, id).await);
    assert_eq!(p, dir.path().join("file (1)"));
    assert_file_eq(&p, 50_000);
    assert_eq!(std::fs::read(dir.path().join("file")).unwrap(), b"keep");
}

#[tokio::test]
async fn same_name_concurrent_downloads() {
    let len = 6 * MIB as usize;
    let mut spec = Spec::new(len);
    spec.bytes_per_sec = Some(8_000_000);
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let a = add(&c, &srv.url, dir.path(), 4, None);
    let b = add(&c, &srv.url, dir.path(), 4, None);
    let (ra, rb) = tokio::join!(run(&c, a), run(&c, b));
    let (pa, pb) = (completed(ra), completed(rb));
    assert_ne!(pa, pb);
    let mut names = vec![pa.clone(), pb.clone()];
    names.sort();
    assert_eq!(names, vec![dir.path().join("file"), dir.path().join("file (1)")]);
    assert_file_eq(&pa, len);
    assert_file_eq(&pb, len);
    assert!(part_files(dir.path()).is_empty());
}

#[tokio::test]
async fn speed_limit_respected() {
    let srv = serve(Spec::new(2_000_000)).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 4, Some(1_000_000));
    let t = Instant::now();
    let p = completed(run(&c, id).await);
    let secs = t.elapsed().as_secs_f64();
    assert!((1.6..=3.0).contains(&secs), "took {secs:.2} s");
    assert_file_eq(&p, 2_000_000);
}

#[tokio::test]
async fn per_host_cap() {
    let len = 32 * MIB as usize;
    let mut spec = Spec::new(len);
    spec.bytes_per_sec = Some(16_000_000);
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(2, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 8, None);
    let p = completed(run(&c, id).await);
    assert!(srv.max_concurrent.load(SeqCst) <= 2, "max_concurrent {}", srv.max_concurrent.load(SeqCst));
    assert_file_eq(&p, len);
}

#[tokio::test]
async fn disk_full_refuses_start() {
    let srv = serve(Spec::new(5_000_000)).await;
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = TransferConfig::for_tests();
    cfg.free_space_override = Some(1000);
    let c = ctx(16, cfg);
    let id = add(&c, &srv.url, dir.path(), 4, None);
    match run(&c, id).await {
        TransferOutcome::Failed(EngineError::DiskFull) => {}
        other => panic!("expected DiskFull, got {other:?}"),
    }
    assert!(part_files(dir.path()).is_empty());
}

#[tokio::test]
async fn progress_events_throttled() {
    let mut spec = Spec::new(3_000_000);
    spec.bytes_per_sec = Some(1_000_000);
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 1, None);
    let mut rx = c.events.subscribe();
    let collector = tokio::spawn(async move {
        let mut times = Vec::new();
        while let Ok(ev) = rx.recv().await {
            match ev {
                Event::Progress { id: pid, .. } if pid == id => times.push(Instant::now()),
                Event::StateChanged { id: pid, state, .. }
                    if pid == id && state == haul_engine::DownloadState::Completed =>
                {
                    break;
                }
                _ => {}
            }
        }
        times
    });
    completed(run(&c, id).await);
    let times = tokio::time::timeout(Duration::from_secs(5), collector).await.unwrap().unwrap();
    assert!(times.len() >= 3, "only {} progress events", times.len());
    for (i, t) in times.iter().enumerate() {
        let in_window = times[i..].iter().take_while(|u| u.duration_since(*t) < Duration::from_secs(1)).count();
        assert!(in_window <= 5, "{in_window} progress events within 1 s");
    }
}

// ---- Task 7: robustness ----

fn steal_count(rx: &mut tokio::sync::broadcast::Receiver<Event>) -> usize {
    let mut n = 0;
    while let Ok(ev) = rx.try_recv() {
        if matches!(ev, Event::Steal { .. }) {
            n += 1;
        }
    }
    n
}

#[tokio::test]
async fn steal_happens() {
    let len = 24 * MIB as usize;
    let mut spec = Spec::new(len);
    spec.slow_from = Some((18 * MIB, 1_000_000));
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let mut rx = c.events.subscribe();
    let id = add(&c, &srv.url, dir.path(), 4, None);
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, len);
    assert!(steal_count(&mut rx) >= 1, "no steal happened");
}

#[tokio::test]
async fn slow_connection_replaced() {
    let len = 24 * MIB as usize;
    let mut spec = Spec::new(len);
    spec.bytes_per_sec = Some(8_000_000);
    spec.slow_from = Some((18 * MIB, 1_000_000));
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 4, None);
    let requests = srv.requests.clone();
    // Before the fast ranges finish (~0.8 s) no steal can happen, so any request
    // beyond HEAD + 4 ranges is a slow connection being reopened.
    let early = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(650)).await;
        requests.load(SeqCst)
    });
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, len);
    let at_650ms = early.await.unwrap();
    assert!(at_650ms > 5, "only {at_650ms} requests by 650 ms: slow connection not replaced");
}

#[tokio::test]
async fn dropped_connection_retried() {
    let len = 6 * MIB as usize;
    let mut spec = Spec::new(len);
    spec.drop_after = Some(300_000);
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 8, None);
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, len);
    // HEAD + the dropped request + at least one retry
    assert!(srv.requests.load(SeqCst) >= 3);
}

#[tokio::test]
async fn retries_exhausted_fails() {
    let mut spec = Spec::new(1000);
    spec.status = Some(503);
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let cfg = TransferConfig::for_tests();
    let max = cfg.max_attempts;
    let c = ctx(16, cfg);
    let id = add(&c, &srv.url, dir.path(), 4, None);
    match run(&c, id).await {
        TransferOutcome::Failed(EngineError::Http(503)) => {}
        other => panic!("expected Http(503), got {other:?}"),
    }
    assert!(srv.requests.load(SeqCst) >= max, "gave up after {} requests", srv.requests.load(SeqCst));
    assert_eq!(c.store.get_download(id).unwrap().state, haul_engine::DownloadState::Failed);
}

#[tokio::test]
async fn expired_mid_download() {
    let len = 20 * MIB as usize;
    let mut spec = Spec::new(len);
    spec.bytes_per_sec = Some(4_000_000);
    spec.drop_after = Some(2 * MIB); // forces fresh requests mid-download
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 4, None);
    let mut rx = c.events.subscribe();
    let spec_handle = srv.spec.clone();
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            if let Event::Progress { written, .. } = ev
                && written >= 5 * MIB
            {
                spec_handle.lock().unwrap().status = Some(403);
                break;
            }
        }
    });
    match run(&c, id).await {
        TransferOutcome::Failed(EngineError::LinkExpired(403)) => {}
        other => panic!("expected LinkExpired(403), got {other:?}"),
    }
    assert_eq!(c.store.get_download(id).unwrap().state, haul_engine::DownloadState::LinkExpired);
    let saved: u64 = c.store.load_segments(id).unwrap().iter().map(|r| r.pos - r.start).sum();
    assert!(saved > 0, "no progress saved");
}

#[tokio::test]
async fn range_ignored_mid_download_falls_back() {
    let len = 16 * MIB as usize;
    let mut spec = Spec::new(len);
    spec.ignore_range_after_requests = Some(2);
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 8, None);
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, len);
    assert!(part_files(dir.path()).is_empty());
}

#[tokio::test]
async fn range_not_satisfiable_fails_after_one_restart() {
    let mut spec = Spec::new(8 * MIB as usize);
    spec.range_status = Some(416);
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 2, None);
    match run(&c, id).await {
        TransferOutcome::Failed(EngineError::RangeNotSatisfiable) => {}
        other => panic!("expected RangeNotSatisfiable, got {other:?}"),
    }
    // probe + first pass, then a fresh probe + second pass
    assert!(srv.requests.load(SeqCst) >= 4, "{} requests: no restart", srv.requests.load(SeqCst));
}

#[tokio::test]
async fn too_many_requests_sheds_connections() {
    // A host that allows 2 open responses: the other connections get 429.
    let len = 32 * MIB as usize;
    let mut spec = Spec::new(len);
    spec.bytes_per_sec = Some(16_000_000);
    spec.max_open = Some(2);
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 8, None);
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, len);
    assert!(srv.too_many.load(SeqCst) > 0, "the server never said 429");
}

#[tokio::test]
async fn too_many_requests_single_connection_retries() {
    // Even the only connection may get a 429; it backs off and retries.
    let mut spec = Spec::new(1_000_000);
    spec.max_open = Some(0);
    let srv = serve(spec).await;
    let spec_handle = srv.spec.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        spec_handle.lock().unwrap().max_open = None;
    });
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = TransferConfig::for_tests();
    cfg.retry_delays = vec![Duration::from_millis(50); 6];
    let c = ctx(16, cfg);
    let id = add(&c, &srv.url, dir.path(), 1, None);
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, 1_000_000);
}

/// Every connection freezes mid-response. They must be replaced after `slow_window`
/// with no bytes, not after the 30 s read timeout.
#[tokio::test]
async fn stalled_connections_replaced() {
    let len = 8 * MIB as usize;
    let mut spec = Spec::new(len);
    spec.stall_after = Some(300_000);
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 2, None);
    let out = tokio::time::timeout(Duration::from_secs(5), run(&c, id)).await.expect("stalled past 5 s");
    assert_file_eq(&completed(out), len);
}

fn add_with_headers(ctx: &TransferCtx, url: &str, dir: &Path, headers: &str) -> DownloadId {
    ctx.store
        .insert_download(&NewDownload {
            package_id: None,
            url: url.to_string(),
            headers_json: Some(headers.to_string()),
            target_dir: dir.to_path_buf(),
            file_name: None,
            priority: Priority::Normal,
            connections: 4,
            speed_limit: None,
        })
        .unwrap()
}

#[tokio::test]
async fn cookie_sent_on_probe_and_connections() {
    let len = MIB as usize;
    let mut spec = Spec::new(len);
    spec.require_header = Some(("cookie".into(), "sid=abc".into()));
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add_with_headers(&c, &srv.url, dir.path(), r#"{"Cookie":"sid=abc"}"#);
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, len);
    assert!(srv.requests.load(SeqCst) >= 2, "probe plus at least one GET");
}

#[tokio::test]
async fn missing_cookie_gives_link_expired() {
    let mut spec = Spec::new(100_000);
    spec.require_header = Some(("cookie".into(), "sid=abc".into()));
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add(&c, &srv.url, dir.path(), 4, None);
    let _ = run(&c, id).await;
    assert_eq!(c.store.get_download(id).unwrap().state, haul_engine::DownloadState::LinkExpired);
}

#[tokio::test]
async fn invalid_header_values_are_dropped() {
    let srv = serve(Spec::new(100_000)).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(16, TransferConfig::for_tests());
    let id = add_with_headers(&c, &srv.url, dir.path(), "{\"Cookie\":\"a=1\nb=2\",\"Referer\":\"https://x.test/\"}");
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, 100_000);
}
