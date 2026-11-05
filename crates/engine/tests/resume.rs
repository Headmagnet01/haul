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
            if let Event::Progress { id: pid, written, .. } = ev {
                if pid == id && written as f64 >= fraction * LEN as f64 {
                    watcher_cancel.cancel();
                    break;
                }
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
                        if let Ok(Event::Progress { id: pid, written, .. }) = ev {
                            if pid == id && written as f64 >= 0.4 * LEN as f64 { break; }
                        }
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
