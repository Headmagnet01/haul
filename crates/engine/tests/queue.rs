mod support;

use haul_engine::{
    DownloadId, DownloadState, Event, EventBus, HostLimiter, NewDownload, Priority, Queue, RateLimiter, Store,
    TransferConfig, TransferCtx, http_client,
};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::{Spec, TestServer, assert_file_eq, serve};

fn ctx_on(store: Store, config: TransferConfig) -> TransferCtx {
    TransferCtx {
        client: http_client(),
        store,
        events: EventBus::new(),
        global_limit: Arc::new(RateLimiter::new(None)),
        hosts: Arc::new(HostLimiter::new(16)),
        config,
    }
}

fn nd(url: &str, dir: &Path, p: Priority) -> NewDownload {
    NewDownload {
        package_id: None,
        url: url.to_string(),
        headers_json: None,
        target_dir: dir.to_path_buf(),
        file_name: None,
        priority: p,
        connections: 2,
        speed_limit: None,
    }
}

async fn slow_server(len: usize, bps: u64) -> TestServer {
    let mut spec = Spec::new(len);
    spec.bytes_per_sec = Some(bps);
    serve(spec).await
}

async fn wait_state(store: &Store, id: DownloadId, want: DownloadState, secs: u64) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let s = store.get_download(id).map(|d| d.state);
        if s.as_ref().ok() == Some(&want) {
            return;
        }
        assert!(Instant::now() < deadline, "id {id}: wanted {want:?}, still {s:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn part_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().ends_with(".haulpart")).count())
        .unwrap_or(0)
}

#[tokio::test]
async fn respects_max_parallel() {
    let srv = slow_server(1_000_000, 2_000_000).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx_on(Store::open_in_memory().unwrap(), TransferConfig::for_tests());
    let mut rx = c.events.subscribe();
    let q = Queue::start(c.clone(), 2);
    let ids: Vec<_> = (0..5).map(|_| q.add(nd(&srv.url, dir.path(), Priority::Normal)).unwrap()).collect();
    let mut running = std::collections::HashSet::new();
    let mut max_running = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let ev = tokio::time::timeout_at(deadline, rx.recv()).await.expect("timed out").unwrap();
        match ev {
            Event::StateChanged { id, state, .. } => {
                if state == DownloadState::Running {
                    running.insert(id);
                } else {
                    running.remove(&id);
                }
                max_running = max_running.max(running.len());
            }
            Event::AllDone => break,
            _ => {}
        }
    }
    assert!(max_running <= 2, "{max_running} running at once");
    for id in ids {
        assert_eq!(c.store.get_download(id).unwrap().state, DownloadState::Completed);
    }
}

#[tokio::test]
async fn priority_goes_first() {
    let srv = slow_server(1_000_000, 2_000_000).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx_on(Store::open_in_memory().unwrap(), TransferConfig::for_tests());
    let mut rx = c.events.subscribe();
    let q = Queue::start(c.clone(), 1);
    let a = q.add(nd(&srv.url, dir.path(), Priority::Normal)).unwrap();
    wait_state(&c.store, a, DownloadState::Running, 5).await;
    let b = q.add(nd(&srv.url, dir.path(), Priority::Normal)).unwrap();
    let cc = q.add(nd(&srv.url, dir.path(), Priority::High)).unwrap();
    let mut started = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while started.len() < 3 {
        let ev = tokio::time::timeout_at(deadline, rx.recv()).await.expect("timed out").unwrap();
        if let Event::StateChanged { id, state: DownloadState::Running, .. } = ev {
            if !started.contains(&id) {
                started.push(id);
            }
        }
    }
    assert_eq!(started, vec![a, cc, b]);
}

#[tokio::test]
async fn pause_frees_slot() {
    let srv = slow_server(20_000_000, 1_000_000).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx_on(Store::open_in_memory().unwrap(), TransferConfig::for_tests());
    let q = Queue::start(c.clone(), 1);
    let a = q.add(nd(&srv.url, dir.path(), Priority::Normal)).unwrap();
    wait_state(&c.store, a, DownloadState::Running, 5).await;
    let b = q.add(nd(&srv.url, dir.path(), Priority::Normal)).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(c.store.get_download(b).unwrap().state, DownloadState::Queued);
    q.pause(a);
    wait_state(&c.store, a, DownloadState::Paused, 5).await;
    wait_state(&c.store, b, DownloadState::Running, 5).await;
    q.shutdown().await;
}

#[test]
fn restart_resumes_interrupted() {
    let server_rt = tokio::runtime::Runtime::new().unwrap();
    let len = 8_000_000;
    let srv = server_rt.block_on(slow_server(len, 2_000_000));
    let root = tempfile::tempdir().unwrap();
    let db = root.path().join("haul.db");
    let dir = root.path().join("dl");

    let id = {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let id = rt.block_on(async {
            let c = ctx_on(Store::open(&db).unwrap(), TransferConfig::for_tests());
            let q = Queue::start(c.clone(), 3);
            let id = q.add(nd(&srv.url, &dir, Priority::Normal)).unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let w: u64 = c.store.load_segments(id).unwrap().iter().map(|r| r.pos - r.start).sum();
                if w as f64 >= 0.3 * len as f64 {
                    break;
                }
                assert!(Instant::now() < deadline, "never reached 30%");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            id
        });
        drop(rt); // simulated crash: no shutdown
        id
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let c = ctx_on(Store::open(&db).unwrap(), TransferConfig::for_tests());
        let _q = Queue::start(c.clone(), 3);
        wait_state(&c.store, id, DownloadState::Completed, 20).await;
        let p = c.store.get_download(id).unwrap().target_path.unwrap();
        assert_file_eq(&p, len);
    });
}

#[tokio::test]
async fn remove_running_deletes_part() {
    let srv = slow_server(20_000_000, 1_000_000).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx_on(Store::open_in_memory().unwrap(), TransferConfig::for_tests());
    let q = Queue::start(c.clone(), 1);
    let a = q.add(nd(&srv.url, dir.path(), Priority::Normal)).unwrap();
    wait_state(&c.store, a, DownloadState::Running, 5).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(part_files(dir.path()), 1);
    q.remove(a);
    let deadline = Instant::now() + Duration::from_secs(5);
    while c.store.get_download(a).is_ok() || part_files(dir.path()) > 0 {
        assert!(Instant::now() < deadline, "row or part file still there");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn disk_full_pauses() {
    let srv = serve(Spec::new(5_000_000)).await;
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = TransferConfig::for_tests();
    cfg.free_space_override = Some(1000);
    let c = ctx_on(Store::open_in_memory().unwrap(), cfg);
    let q = Queue::start(c.clone(), 1);
    let a = q.add(nd(&srv.url, dir.path(), Priority::Normal)).unwrap();
    wait_state(&c.store, a, DownloadState::Paused, 5).await;
    assert_eq!(c.store.get_download(a).unwrap().error.as_deref(), Some("Disk full"));
}

#[tokio::test]
async fn all_done_emitted_once() {
    let srv = serve(Spec::new(200_000)).await;
    let dir = tempfile::tempdir().unwrap();
    let c = ctx_on(Store::open_in_memory().unwrap(), TransferConfig::for_tests());
    let mut rx = c.events.subscribe();
    let q = Queue::start(c.clone(), 3);
    let a = q.add(nd(&srv.url, dir.path(), Priority::Normal)).unwrap();
    let b = q.add(nd(&srv.url, dir.path(), Priority::Normal)).unwrap();
    wait_state(&c.store, a, DownloadState::Completed, 10).await;
    wait_state(&c.store, b, DownloadState::Completed, 10).await;
    tokio::time::sleep(Duration::from_millis(2500)).await; // a couple of dispatcher ticks
    let mut count = 0;
    while let Ok(ev) = rx.try_recv() {
        if matches!(ev, Event::AllDone) {
            count += 1;
        }
    }
    assert_eq!(count, 1);
}

#[tokio::test]
async fn set_max_parallel_clamps() {
    let c = ctx_on(Store::open_in_memory().unwrap(), TransferConfig::for_tests());
    let q = Queue::start(c, 3);
    assert_eq!(q.max_parallel(), 3);
    q.set_max_parallel(0);
    assert_eq!(q.max_parallel(), 1);
    q.set_max_parallel(50);
    assert_eq!(q.max_parallel(), 20);
}
