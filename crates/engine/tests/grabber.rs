mod support;

use chrono::NaiveTime;
use haul_engine::{
    CollectorState, DownloadState, DupKind, EngineError, EventBus, Grabber, HostLimiter, LinkStatus, NewDownload,
    Priority, ProbeFn, ProbeInfo, Queue, RateLimiter, Schedule, StartDefaults, Store, TransferConfig, TransferCtx,
    http_client, http_prober,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
use std::time::{Duration, Instant};
use support::{Spec, serve};

fn info(url: &str) -> ProbeInfo {
    ProbeInfo {
        final_url: url.to_string(),
        size: Some(1_000_000),
        resumable: true,
        file_name: url.rsplit('/').next().unwrap_or_default().to_string(),
        etag: None,
        last_modified: None,
        content_type: None,
    }
}

fn online(url: &str) -> Result<ProbeInfo, EngineError> {
    Ok(info(url))
}

fn offline_if_404(url: &str) -> Result<ProbeInfo, EngineError> {
    if url.contains("404") { Err(EngineError::Offline) } else { Ok(info(url)) }
}

/// A prober that answers with `map` after `delay_ms`, counting the most probes ever in flight at once.
fn fake(map: fn(&str) -> Result<ProbeInfo, EngineError>, delay_ms: u64) -> (ProbeFn, Arc<AtomicU32>) {
    let now = Arc::new(AtomicU32::new(0));
    let max = Arc::new(AtomicU32::new(0));
    let m = max.clone();
    let f: ProbeFn = Arc::new(move |url: String| {
        let (now, max) = (now.clone(), m.clone());
        Box::pin(async move {
            let n = now.fetch_add(1, SeqCst) + 1;
            max.fetch_max(n, SeqCst);
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            now.fetch_sub(1, SeqCst);
            map(&url)
        })
    });
    (f, max)
}

fn grabber(probe: ProbeFn) -> (Grabber, Store) {
    let store = Store::open_in_memory().unwrap();
    (Grabber::new(store.clone(), EventBus::new(), probe), store)
}

async fn wait_until(g: &Grabber, what: &str, ok: impl Fn(&CollectorState) -> bool) -> CollectorState {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let s = g.state().unwrap();
        if ok(&s) {
            return s;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}: {s:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn all_online(s: &CollectorState) -> bool {
    !s.links.is_empty() && s.links.iter().all(|l| l.status != LinkStatus::Checking)
}

fn links(urls: &[&str]) -> Vec<String> {
    urls.iter().map(|u| u.to_string()).collect()
}

fn idle_queue(store: &Store) -> Queue {
    let ctx = TransferCtx {
        client: http_client(),
        store: store.clone(),
        events: EventBus::new(),
        global_limit: Arc::new(RateLimiter::new(None)),
        hosts: Arc::new(HostLimiter::new(16)),
        config: TransferConfig::default(),
    };
    let q = Queue::start(ctx, 3);
    let t = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    q.set_schedule(Some(Schedule { start: t, stop: t, weekdays: vec![] }));
    q
}

fn defaults() -> StartDefaults {
    StartDefaults { target_dir: PathBuf::from(r"C:\dl"), connections: 8 }
}

#[tokio::test]
async fn intake_creates_checking_links_then_online() {
    let (p, _) = fake(online, 100);
    let (g, _) = grabber(p);
    let r = g.intake(links(&["https://a.org/1.bin", "https://a.org/2.bin"]), "paste").unwrap();
    assert_eq!((r.added, r.skipped), (2, 0));
    let s = g.state().unwrap();
    assert!(s.links.iter().all(|l| l.status == LinkStatus::Checking), "{s:?}");
    let s = wait_until(&g, "online", all_online).await;
    assert!(s.links.iter().all(|l| l.status == LinkStatus::Online && l.size == Some(1_000_000)));
    assert!(s.links.iter().all(|l| l.source == "paste"));
}

#[tokio::test]
async fn probes_at_most_8_at_once() {
    let (p, max) = fake(online, 50);
    let (g, _) = grabber(p);
    let urls: Vec<String> = (0..20).map(|i| format!("https://a.org/{i}.bin")).collect();
    g.intake(urls, "paste").unwrap();
    let s = wait_until(&g, "all online", |s| s.links.len() == 20 && all_online(s)).await;
    assert!(s.links.iter().all(|l| l.status == LinkStatus::Online));
    let m = max.load(SeqCst);
    assert!((2..=8).contains(&m), "max concurrent probes: {m}");
}

#[tokio::test]
async fn intake_skips_urls_already_waiting() {
    let (p, _) = fake(online, 0);
    let (g, store) = grabber(p);
    let two = links(&["https://a.org/1.bin", "https://a.org/2.bin"]);
    g.intake(two.clone(), "clipboard").unwrap();
    let r = g.intake(two, "paste").unwrap();
    assert_eq!((r.added, r.skipped), (0, 2));
    assert_eq!(store.list_batches().unwrap().len(), 1);
    assert_eq!(g.state().unwrap().links.len(), 2);
}

#[tokio::test]
async fn state_groups_archive_parts() {
    let (p, _) = fake(online, 0);
    let (g, _) = grabber(p);
    g.intake(
        links(&[
            "https://a.org/m.7z.002",
            "https://a.org/m.7z.001",
            "https://a.org/notes.txt",
            "https://a.org/other.bin",
        ]),
        "paste",
    )
    .unwrap();
    let s = wait_until(&g, "online", all_online).await;
    assert_eq!(s.packages.len(), 2, "{s:?}");
    assert_eq!(s.packages[0].name, "m");
    assert_eq!(s.packages[0].archive, Some(haul_engine::ArchiveKind::SevenZ));
    assert!(s.packages[1].name.starts_with("Pasted list · "), "{}", s.packages[1].name);
    assert_eq!(s.packages[1].archive, None);
    let names: Vec<&str> = s.links.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, vec!["m.7z.001", "m.7z.002", "notes.txt", "other.bin"]);
    assert_eq!(s.links[0].package_key.as_ref(), Some(&s.packages[0].key));
    assert_eq!(s.links[3].package_key.as_ref(), Some(&s.packages[1].key));
}

#[tokio::test]
async fn single_link_has_no_package() {
    let (p, _) = fake(online, 0);
    let (g, _) = grabber(p);
    g.intake(links(&["https://a.org/one.bin"]), "paste").unwrap();
    let s = wait_until(&g, "online", all_online).await;
    assert!(s.packages.is_empty());
    assert_eq!(s.links[0].package_key, None);
}

#[tokio::test]
async fn rest_named_by_prefix() {
    let (p, _) = fake(online, 0);
    let (g, _) = grabber(p);
    g.intake(links(&["https://a.org/godot-4.4-stable-win64.zip", "https://a.org/godot-4.4-stable-linux.zip"]), "paste")
        .unwrap();
    let s = wait_until(&g, "online", all_online).await;
    assert_eq!(s.packages.len(), 1);
    assert_eq!(s.packages[0].name, "godot-4.4-stable");
}

#[tokio::test]
async fn duplicates_flagged() {
    let (p, _) = fake(online, 0);
    let (g, store) = grabber(p);
    let done = store
        .insert_download(&NewDownload {
            package_id: None,
            url: "https://a.org/done.bin".into(),
            headers_json: None,
            target_dir: PathBuf::from(r"C:\dl"),
            file_name: None,
            priority: Priority::Normal,
            connections: 8,
            speed_limit: None,
        })
        .unwrap();
    store.set_state(done, DownloadState::Completed, None).unwrap();
    g.intake(links(&["https://a.org/done.bin"]), "paste").unwrap();
    g.intake(links(&["https://a.org/x/same.bin"]), "paste").unwrap();
    g.intake(links(&["https://b.org/y/same.bin"]), "clipboard").unwrap();
    let s = wait_until(&g, "online", all_online).await;
    let dup = |name: &str, url: &str| s.links.iter().find(|l| l.name == name && l.url == url).unwrap().duplicate;
    assert_eq!(dup("done.bin", "https://a.org/done.bin"), Some(DupKind::Downloaded));
    assert_eq!(dup("same.bin", "https://a.org/x/same.bin"), None);
    assert_eq!(dup("same.bin", "https://b.org/y/same.bin"), Some(DupKind::Collector));
}

#[tokio::test]
async fn start_queues_in_part_order_with_package() {
    let (p, _) = fake(online, 0);
    let (g, store) = grabber(p);
    let q = idle_queue(&store);
    g.intake(
        links(&[
            "https://a.org/m.7z.002",
            "https://a.org/m.7z.001",
            "https://a.org/notes.txt",
            "https://a.org/other.bin",
        ]),
        "paste",
    )
    .unwrap();
    let s = wait_until(&g, "online", all_online).await;
    let key = s.packages[0].key.clone();
    let ids: Vec<i64> = s.links.iter().filter(|l| l.package_key.as_ref() == Some(&key)).map(|l| l.id).collect();
    assert_eq!(g.start(&ids, &q, &defaults()).unwrap(), 2);
    let ds = store.list_downloads().unwrap();
    assert_eq!(ds.len(), 2);
    assert!(ds[0].url.ends_with("m.7z.001") && ds[1].url.ends_with("m.7z.002"), "{ds:?}");
    assert!(ds[0].package_id.is_some() && ds[0].package_id == ds[1].package_id);
    assert_eq!(ds[0].probe.as_ref().and_then(|p| p.size), Some(1_000_000));
    assert_eq!(ds[0].target_dir, PathBuf::from(r"C:\dl"));
    assert!(store.list_packages().unwrap().iter().any(|p| p.name == "m" && Some(p.id) == ds[0].package_id));
    let left: Vec<String> = g.state().unwrap().links.into_iter().map(|l| l.name).collect();
    assert_eq!(left, vec!["notes.txt", "other.bin"]);
    q.shutdown().await;
}

#[tokio::test]
async fn start_again_reuses_package() {
    let (p, _) = fake(online, 0);
    let (g, store) = grabber(p);
    let q = idle_queue(&store);
    g.intake(links(&["https://a.org/m.7z.001", "https://a.org/m.7z.002"]), "paste").unwrap();
    let s = wait_until(&g, "online", all_online).await;
    assert_eq!(g.start(&[s.links[0].id], &q, &defaults()).unwrap(), 1);
    assert_eq!(g.start(&[s.links[1].id], &q, &defaults()).unwrap(), 1);
    let ds = store.list_downloads().unwrap();
    assert_eq!(ds.len(), 2);
    assert!(ds[0].package_id.is_some() && ds[0].package_id == ds[1].package_id);
    q.shutdown().await;
}

#[tokio::test]
async fn start_skips_offline() {
    let (p, _) = fake(offline_if_404, 0);
    let (g, store) = grabber(p);
    let q = idle_queue(&store);
    g.intake(links(&["https://a.org/404.bin"]), "paste").unwrap();
    let s = wait_until(&g, "checked", all_online).await;
    assert_eq!(s.links[0].status, LinkStatus::Offline);
    assert_eq!(s.links[0].message.as_deref(), Some("404"));
    assert_eq!(g.start(&[s.links[0].id], &q, &defaults()).unwrap(), 0);
    assert_eq!(g.state().unwrap().links.len(), 1);
    assert!(store.list_downloads().unwrap().is_empty());
    q.shutdown().await;
}

#[tokio::test]
async fn probe_finishing_after_remove_is_ignored() {
    let gate = Arc::new(tokio::sync::Notify::new());
    let g2 = gate.clone();
    let p: ProbeFn = Arc::new(move |url: String| {
        let gate = g2.clone();
        Box::pin(async move {
            gate.notified().await;
            Ok(info(&url))
        })
    });
    let (g, store) = grabber(p);
    g.intake(links(&["https://a.org/1.bin"]), "paste").unwrap();
    let id = g.state().unwrap().links[0].id;
    tokio::time::sleep(Duration::from_millis(50)).await;
    g.remove(&[id]).unwrap();
    gate.notify_waiters();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(g.state().unwrap().links.is_empty());
    assert!(store.list_batches().unwrap().is_empty());
}

#[tokio::test]
async fn checking_links_reprobed_on_start() {
    let store = Store::open_in_memory().unwrap();
    let b = store.create_batch("paste", "Pasted list · 5 Nov 18:40").unwrap();
    store.insert_link(b, "https://a.org/left.bin").unwrap();
    let (p, _) = fake(online, 0);
    let g = Grabber::new(store, EventBus::new(), p);
    let s = wait_until(&g, "online", all_online).await;
    assert_eq!(s.links[0].status, LinkStatus::Online);
}

#[tokio::test]
async fn recheck_reprobes_error_links() {
    let calls = Arc::new(AtomicU32::new(0));
    let c = calls.clone();
    let p: ProbeFn = Arc::new(move |url: String| {
        let n = c.fetch_add(1, SeqCst);
        Box::pin(async move { if n == 0 { Err(EngineError::Network("timed out".into())) } else { Ok(info(&url)) } })
    });
    let (g, _) = grabber(p);
    g.intake(links(&["https://a.org/1.bin"]), "paste").unwrap();
    let s = wait_until(&g, "checked", all_online).await;
    assert_eq!(s.links[0].status, LinkStatus::Error);
    g.recheck(&[s.links[0].id]).unwrap();
    assert_eq!(g.state().unwrap().links[0].status, LinkStatus::Checking);
    let s = wait_until(&g, "online", |s| s.links[0].status == LinkStatus::Online).await;
    assert_eq!(s.links[0].message, None);
}

#[tokio::test]
async fn http_prober_online_and_404() {
    let ok = serve(Spec::new(2_000_000)).await;
    let mut gone = Spec::new(10);
    gone.status = Some(404);
    let gone = serve(gone).await;
    let (g, _) = grabber(http_prober(http_client()));
    g.intake(vec![ok.url.clone()], "paste").unwrap();
    g.intake(vec![gone.url.clone()], "paste").unwrap();
    let s = wait_until(&g, "checked", |s| s.links.len() == 2 && all_online(s)).await;
    let by = |u: &str| s.links.iter().find(|l| l.url == u).unwrap();
    assert_eq!(by(&ok.url).status, LinkStatus::Online);
    assert_eq!(by(&ok.url).size, Some(2_000_000));
    assert_eq!(by(&gone.url).status, LinkStatus::Offline);
    assert_eq!(by(&gone.url).message.as_deref(), Some("404"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_starts_queue_each_link_once() {
    let (p, _) = fake(online, 0);
    let (g, store) = grabber(p);
    let q = idle_queue(&store);
    let urls: Vec<String> = (0..60).map(|i| format!("https://a.org/m.part{i}.rar")).collect();
    g.intake(urls, "paste").unwrap();
    let s = wait_until(&g, "online", |s| s.links.len() == 60 && all_online(s)).await;
    let ids: Vec<i64> = s.links.iter().map(|l| l.id).collect();
    let started: usize = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..4).map(|_| sc.spawn(|| g.start(&ids, &q, &defaults()).unwrap())).collect();
        hs.into_iter().map(|h| h.join().unwrap()).sum()
    });
    assert_eq!(started, 60);
    assert_eq!(store.list_downloads().unwrap().len(), 60);
    q.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_intakes_add_each_link_once() {
    let (p, _) = fake(online, 0);
    let (g, store) = grabber(p);
    let urls: Vec<String> = (0..300).map(|i| format!("https://a.org/{i}.bin")).collect();
    let added: usize = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..4).map(|_| sc.spawn(|| g.intake(urls.clone(), "paste").unwrap().added)).collect();
        hs.into_iter().map(|h| h.join().unwrap()).sum()
    });
    assert_eq!(added, 300);
    assert_eq!(store.list_links().unwrap().len(), 300);
}

#[tokio::test]
async fn archive_parts_from_separate_intakes_join() {
    let (p, _) = fake(online, 0);
    let (g, _) = grabber(p);
    g.intake(links(&["https://a.org/movie.part1.rar"]), "clipboard").unwrap();
    g.intake(links(&["https://a.org/other.bin"]), "clipboard").unwrap();
    g.intake(links(&["https://b.org/Movie.part2.rar"]), "clipboard").unwrap();
    let s = wait_until(&g, "online", |s| s.links.len() == 3 && all_online(s)).await;
    assert_eq!(s.packages.len(), 1, "{s:?}");
    assert_eq!(s.packages[0].name, "movie");
    let names: Vec<&str> = s.links.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, vec!["movie.part1.rar", "Movie.part2.rar", "other.bin"]);
    assert_eq!(s.links[1].package_key.as_ref(), Some(&s.packages[0].key));
    assert_eq!(s.links[2].package_key, None);
}

#[tokio::test]
async fn late_part_joins_started_package() {
    let (p, _) = fake(online, 0);
    let (g, store) = grabber(p);
    let q = idle_queue(&store);
    g.intake(links(&["https://a.org/m.part1.rar", "https://a.org/m.part2.rar"]), "paste").unwrap();
    let s = wait_until(&g, "online", all_online).await;
    let ids: Vec<i64> = s.links.iter().map(|l| l.id).collect();
    assert_eq!(g.start(&ids, &q, &defaults()).unwrap(), 2);
    g.intake(links(&["https://a.org/m.part3.rar"]), "clipboard").unwrap();
    let s = wait_until(&g, "online", all_online).await;
    assert_eq!(g.start(&[s.links[0].id], &q, &defaults()).unwrap(), 1);
    let ds = store.list_downloads().unwrap();
    assert_eq!(ds.len(), 3);
    assert!(ds.iter().all(|d| d.package_id.is_some() && d.package_id == ds[0].package_id), "{ds:?}");
    q.shutdown().await;
}
