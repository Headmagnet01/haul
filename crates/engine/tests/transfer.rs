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
