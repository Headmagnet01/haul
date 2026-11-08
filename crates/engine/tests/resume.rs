mod support;

use haul_engine::{
    DownloadId, Event, EventBus, HostLimiter, MIB, NewDownload, Priority, RateLimiter, Store, TransferConfig,
    TransferCtx, TransferJob, TransferOutcome, http_client, run_transfer,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering::SeqCst;
use support::{Spec, TestServer, assert_file_eq, serve};
use tokio_util::sync::CancellationToken;

const LEN: usize = 20 * MIB as usize;

fn ctx_on(store: Store) -> TransferCtx {
    TransferCtx {
        client: http_client(),
        store,
        events: EventBus::new(),
        global_limit: Arc::new(RateLimiter::new(None)),
        hosts: Arc::new(HostLimiter::new(16)),
        config: TransferConfig::for_tests(),
    }
}

fn add(ctx: &TransferCtx, url: &str, dir: &Path) -> DownloadId {
    ctx.store
        .insert_download(&NewDownload {
            package_id: None,
            url: url.to_string(),
            headers_json: None,
            target_dir: dir.to_path_buf(),
            file_name: None,
            priority: Priority::Normal,
            connections: 4,
            speed_limit: None,
        })
        .unwrap()
}

async fn throttled_server() -> TestServer {
    let mut spec = Spec::new(LEN);
    spec.bytes_per_sec = Some(4_000_000);
    spec.etag = Some("v1".into());
    serve(spec).await
}

/// Runs the transfer and cancels it once `fraction` of LEN has been written.
async fn run_until(ctx: &TransferCtx, id: DownloadId, fraction: f64) -> TransferOutcome {
    let cancel = CancellationToken::new();
    let mut rx = ctx.events.subscribe();
    let watcher_cancel = cancel.clone();
    let watcher = tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            if let Event::Progress { id: pid, written, .. } = ev
                && pid == id
                && written as f64 >= fraction * LEN as f64
            {
                watcher_cancel.cancel();
                break;
            }
        }
    });
    let out = run_transfer(TransferJob { id, cancel }, ctx).await;
    watcher.abort();
    out
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

fn written(ctx: &TransferCtx, id: DownloadId) -> u64 {
    ctx.store.load_segments(id).unwrap().iter().map(|r| r.pos - r.start).sum()
}

fn has_part_file(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).any(|e| e.file_name().to_string_lossy().ends_with(".haulpart")))
        .unwrap_or(false)
}

#[tokio::test]
async fn pause_then_resume_completes() {
    let srv = throttled_server().await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx_on(Store::open_in_memory().unwrap());
    let id = add(&c, &srv.url, dir.path());
    let out = run_until(&c, id, 0.4).await;
    assert!(matches!(out, TransferOutcome::Paused), "got {out:?}");
    assert_eq!(c.store.get_download(id).unwrap().state, haul_engine::DownloadState::Paused);
    assert!(has_part_file(dir.path()));
    let done = written(&c, id);
    assert!(done as f64 >= 0.3 * LEN as f64, "only {done} saved");
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, LEN);
    let sent = srv.bytes_sent.load(SeqCst);
    assert!((sent as f64) < 1.5 * LEN as f64, "sent {sent}: restarted instead of resuming");
}

#[tokio::test]
async fn etag_change_restarts() {
    let srv = throttled_server().await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx_on(Store::open_in_memory().unwrap());
    let id = add(&c, &srv.url, dir.path());
    assert!(matches!(run_until(&c, id, 0.4).await, TransferOutcome::Paused));
    srv.spec.lock().unwrap().etag = Some("v2".into());
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, LEN);
    let sent = srv.bytes_sent.load(SeqCst);
    assert!((sent as f64) >= 1.4 * LEN as f64, "sent {sent}: resumed a changed file");
}

#[tokio::test]
async fn resume_with_missing_part_file_restarts() {
    let srv = throttled_server().await;
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("sub");
    let c = ctx_on(Store::open_in_memory().unwrap());
    let id = add(&c, &srv.url, &dir);
    assert!(matches!(run_until(&c, id, 0.4).await, TransferOutcome::Paused));
    std::fs::remove_dir_all(&dir).unwrap();
    let p = completed(run(&c, id).await);
    assert_eq!(p, dir.join("file"));
    assert_file_eq(&p, LEN);
}

#[test]
fn crash_simulation() {
    // The server lives in its own runtime so it survives the "crash".
    let server_rt = tokio::runtime::Runtime::new().unwrap();
    let srv = server_rt.block_on(throttled_server());
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("dl");
    let db = root.path().join("haul.db");

    let id = {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let id = rt.block_on(async {
            let c = ctx_on(Store::open(&db).unwrap());
            let id = add(&c, &srv.url, &dir);
            let mut rx = c.events.subscribe();
            let job = run_transfer(TransferJob { id, cancel: CancellationToken::new() }, &c);
            tokio::pin!(job);
            loop {
                tokio::select! {
                    out = &mut job => panic!("finished before the crash: {out:?}"),
                    ev = rx.recv() => {
                        if let Ok(Event::Progress { id: pid, written, .. }) = ev
                            && pid == id && written as f64 >= 0.4 * LEN as f64 { break; }
                    }
                }
            }
            // Give the 50 ms saver a moment, then crash: no pause, no flush.
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
            id
        });
        drop(rt);
        id
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let c = ctx_on(Store::open(&db).unwrap());
        assert!(written(&c, id) > 0, "nothing was saved before the crash");
        let p = completed(run(&c, id).await);
        assert_file_eq(&p, LEN);
    });
}

fn ctx_with_free_space(store: Store, free: u64) -> TransferCtx {
    let mut c = ctx_on(store);
    c.config.free_space_override = Some(free);
    c
}

/// A changed file must restart from zero even if the first attempt after the change
/// fails before any progress is saved (here: another program holds the part file locked).
#[cfg(windows)]
#[tokio::test]
async fn changed_file_restarts_after_early_failure() {
    use std::os::windows::fs::OpenOptionsExt;
    let srv = throttled_server().await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx_on(Store::open_in_memory().unwrap());
    let id = add(&c, &srv.url, dir.path());
    assert!(matches!(run_until(&c, id, 0.4).await, TransferOutcome::Paused));
    srv.spec.lock().unwrap().etag = Some("v2".into());
    let part = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().ends_with(".haulpart"))
        .unwrap()
        .path();
    let lock = std::fs::OpenOptions::new().read(true).share_mode(0).open(&part).unwrap();
    let out = run(&c, id).await;
    assert!(matches!(out, TransferOutcome::Failed(haul_engine::EngineError::Io(_))), "got {out:?}");
    drop(lock);
    let before = srv.bytes_sent.load(SeqCst);
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, LEN);
    let sent = srv.bytes_sent.load(SeqCst) - before;
    assert!(sent as f64 >= 0.95 * LEN as f64, "sent {sent}: resumed stale bytes of a changed file");
}

/// The part file already holds its full size on disk, so resuming needs no new space.
#[tokio::test]
async fn resume_needs_no_space_for_allocated_part() {
    let srv = throttled_server().await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx_on(Store::open_in_memory().unwrap());
    let id = add(&c, &srv.url, dir.path());
    assert!(matches!(run_until(&c, id, 0.4).await, TransferOutcome::Paused));
    let tight = ctx_with_free_space(c.store.clone(), (LEN / 4) as u64);
    let p = completed(run(&tight, id).await);
    assert_file_eq(&p, LEN);
}

/// A resume with one big range left still uses all its connections (the extras steal).
#[tokio::test]
async fn resume_with_one_range_uses_all_connections() {
    let srv = throttled_server().await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx_on(Store::open_in_memory().unwrap());
    let id = add(&c, &srv.url, dir.path());
    assert!(matches!(run_until(&c, id, 0.05).await, TransferOutcome::Paused));
    c.store.save_segments(id, &[haul_engine::Range { start: 0, pos: 0, end: LEN as u64 }]).unwrap();
    srv.max_concurrent.store(0, SeqCst);
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, LEN);
    let max = srv.max_concurrent.load(SeqCst);
    assert!(max >= 3, "only {max} connections at once");
}

/// Disk full leaves the download Paused, and says so in one state change.
#[tokio::test]
async fn disk_full_is_a_pause() {
    let srv = throttled_server().await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx_with_free_space(Store::open_in_memory().unwrap(), 1000);
    let id = add(&c, &srv.url, dir.path());
    let mut rx = c.events.subscribe();
    let out = run(&c, id).await;
    assert!(matches!(out, TransferOutcome::Failed(haul_engine::EngineError::DiskFull)), "got {out:?}");
    assert_eq!(c.store.get_download(id).unwrap().state, haul_engine::DownloadState::Paused);
    let mut finals = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        if let Event::StateChanged { id: e, state, .. } = ev
            && e == id
            && state != haul_engine::DownloadState::Running
        {
            finals.push(state);
        }
    }
    assert_eq!(finals, vec![haul_engine::DownloadState::Paused]);
}

#[tokio::test]
async fn cookie_sent_on_resume() {
    let mut spec = Spec::new(LEN);
    spec.bytes_per_sec = Some(4_000_000);
    spec.etag = Some("v1".into());
    spec.require_header = Some(("cookie".into(), "sid=abc".into()));
    let srv = serve(spec).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx_on(Store::open_in_memory().unwrap());
    let id = c
        .store
        .insert_download(&NewDownload {
            package_id: None,
            url: srv.url.clone(),
            headers_json: Some(r#"{"Cookie":"sid=abc"}"#.into()),
            target_dir: dir.path().to_path_buf(),
            file_name: None,
            priority: Priority::Normal,
            connections: 4,
            speed_limit: None,
        })
        .unwrap();
    assert!(matches!(run_until(&c, id, 0.2).await, TransferOutcome::Paused));
    assert!(written(&c, id) > 0);
    let p = completed(run(&c, id).await);
    assert_file_eq(&p, LEN);
}
