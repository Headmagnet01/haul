//! Post-processing runs end to end: real 7-Zip and fixtures, a fake Recycle Bin, scanner and notifier.

mod support;

use haul_engine::postproc::extract::{SevenZip, seven_zip_for_tests};
use haul_engine::postproc::scan::{ScanOutcome, Scanner};
use haul_engine::postproc::sort::Recycler;
use haul_engine::postproc::{Notice, PostProc, PostProcDeps, PostProcSettings, Run, RunState, StepKind, StepState};
use haul_engine::rules::{Rule, RuleAction, RuleMatch};
use haul_engine::{
    DownloadId, DownloadState, Event, EventBus, HostLimiter, NewDownload, PackageId, Priority, Queue, RateLimiter,
    Store, TransferConfig, TransferCtx, http_client,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use support::{Spec, body, serve};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

/// Moves "recycled" files into `<dl>/recycled/` and remembers them.
struct FakeBin {
    dir: PathBuf,
    got: Mutex<Vec<PathBuf>>,
}

impl Recycler for FakeBin {
    fn recycle(&self, paths: &[PathBuf]) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        for p in paths {
            let to = self.dir.join(format!(
                "{}-{}",
                self.got.lock().unwrap().len(),
                p.file_name().unwrap().to_string_lossy()
            ));
            std::fs::rename(p, to).map_err(|e| e.to_string())?;
            self.got.lock().unwrap().push(p.clone());
        }
        Ok(())
    }
}

struct CleanScan;
impl Scanner for CleanScan {
    fn scan(&self, _: &Path) -> ScanOutcome {
        ScanOutcome::Clean
    }
}

struct H {
    store: Store,
    events: EventBus,
    pp: PostProc,
    dl: PathBuf,
    bin: Arc<FakeBin>,
    notices: Arc<Mutex<Vec<Notice>>>,
    _tmp: tempfile::TempDir,
}

fn harness_with(store: Store, tmp: tempfile::TempDir) -> H {
    let dl = tmp.path().join("dl");
    std::fs::create_dir_all(&dl).unwrap();
    let mut config = TransferConfig::default();
    config.retry_delays = vec![Duration::from_millis(10)];
    config.max_attempts = 1;
    let ctx = TransferCtx {
        client: http_client(),
        store: store.clone(),
        events: EventBus::new(),
        global_limit: Arc::new(RateLimiter::new(None)),
        hosts: Arc::new(HostLimiter::new(16)),
        config,
    };
    let events = ctx.events.clone();
    let queue = Queue::start(ctx, 3);
    let bin = Arc::new(FakeBin { dir: tmp.path().join("recycled"), got: Mutex::new(Vec::new()) });
    let notices = Arc::new(Mutex::new(Vec::new()));
    let n2 = notices.clone();
    let deps = PostProcDeps {
        extractor: Arc::new(SevenZip { exe: seven_zip_for_tests() }),
        recycler: bin.clone(),
        scanner: Arc::new(CleanScan),
        notify: Arc::new(move |n| n2.lock().unwrap().push(n)),
    };
    let pp = PostProc::start(store.clone(), events.clone(), queue, deps, dl.clone());
    H { store, events, pp, dl, bin, notices, _tmp: tmp }
}

fn harness() -> H {
    harness_with(Store::open_in_memory().unwrap(), tempfile::tempdir().unwrap())
}

fn nd(pkg: Option<PackageId>, url: &str, dir: &Path) -> NewDownload {
    NewDownload {
        package_id: pkg,
        url: url.into(),
        headers_json: None,
        target_dir: dir.to_path_buf(),
        file_name: None,
        priority: Priority::Normal,
        connections: 1,
        speed_limit: None,
    }
}

impl H {
    /// A finished download of a fixture file, without telling anyone yet.
    fn finished(&self, pkg: Option<PackageId>, fixture_path: &str) -> DownloadId {
        let src = fixture(fixture_path);
        let name = src.file_name().unwrap().to_string_lossy().into_owned();
        let id = self.store.insert_download(&nd(pkg, &format!("https://files.example.org/{name}"), &self.dl)).unwrap();
        self.store.set_file_name(id, &name).unwrap();
        let path = self.dl.join(&name);
        std::fs::copy(&src, &path).unwrap();
        self.store.set_completed(id, &path).unwrap();
        id
    }

    fn complete(&self, id: DownloadId) {
        self.events.send(Event::StateChanged { id, state: DownloadState::Completed, error: None });
    }

    fn finish(&self, pkg: Option<PackageId>, fixture_path: &str) -> DownloadId {
        let id = self.finished(pkg, fixture_path);
        self.complete(id);
        id
    }

    fn package(&self, name: &str, fixtures: &[&str]) -> (PackageId, Vec<DownloadId>) {
        let pkg = self.store.create_package(name, "paste").unwrap();
        let ids: Vec<_> = fixtures.iter().map(|f| self.finished(Some(pkg), f)).collect();
        for id in &ids {
            self.complete(*id);
        }
        (pkg, ids)
    }

    async fn wait(&self, what: &str, f: impl Fn(&[Run]) -> bool) -> Vec<Run> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let runs = self.pp.runs().unwrap();
            if f(&runs) {
                return runs;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}: {runs:#?}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn settled(&self) -> Run {
        let runs = self
            .wait("a settled run", |r| {
                r.len() == 1
                    && matches!(
                        r[0].state,
                        RunState::Done | RunState::Failed | RunState::NeedsPassword | RunState::Held
                    )
            })
            .await;
        runs[0].clone()
    }

    async fn settled_again(&self, after: &str) -> Run {
        let runs = self
            .wait("a run settled after an action", |r| {
                r.len() == 1
                    && r[0].updated_at.as_str() > after
                    && matches!(r[0].state, RunState::Done | RunState::Failed | RunState::NeedsPassword)
            })
            .await;
        runs[0].clone()
    }
}

fn step(r: &Run, k: StepKind) -> (StepState, String) {
    let s = r.steps.iter().find(|s| s.kind == k).unwrap();
    (s.state, s.detail.clone().unwrap_or_default())
}

fn states(r: &Run) -> Vec<StepState> {
    r.steps.iter().map(|s| s.state).collect()
}

#[tokio::test]
async fn split_7z_package_extracts_and_sorts() {
    let h = harness();
    h.package("split", &["archives/split.7z.001", "archives/split.7z.002", "archives/split.7z.003"]);
    let r = h.settled().await;
    assert_eq!(r.state, RunState::Done, "{r:#?}");
    use StepState::*;
    assert_eq!(states(&r), vec![Skipped, Done, Done, Skipped, Done]);
    assert_eq!(step(&r, StepKind::Verify).1, "No checksum");
    assert_eq!(step(&r, StepKind::Scan).1, "Off");
    assert_eq!(std::fs::read_to_string(h.dl.join(r"Other\split\hello.txt")).unwrap(), "hello haul\n");
    assert!(h.dl.join(r"Compressed\split.7z.001").is_file());
    assert!(!h.dl.join("split.7z.001").exists());
    let n = h.notices.lock().unwrap();
    assert_eq!(n.len(), 1);
    assert_eq!(n[0].title, "split is ready");
}

#[tokio::test]
async fn checksum_file_verifies() {
    let h = harness();
    h.package("plain", &["archives/plain.zip", "archives/SHA256SUMS"]);
    let r = h.settled().await;
    assert_eq!(r.state, RunState::Done, "{r:#?}");
    assert_eq!(step(&r, StepKind::Verify), (StepState::Done, "SHA-256 from SHA256SUMS".into()));
    // The checksum file travels with the archive it describes.
    assert!(h.dl.join(r"Compressed\SHA256SUMS").is_file());
}

#[tokio::test]
async fn pasted_hash_verifies() {
    let h = harness();
    let id = h.finished(None, "src/hello.txt");
    h.pp.set_expected_hash(id, Some("md5:0dd8c03ef74686e11693dcffbb7a9417")).unwrap();
    h.complete(id);
    let r = h.settled().await;
    assert_eq!(step(&r, StepKind::Verify), (StepState::Done, "MD5 from your checksum".into()));
}

#[tokio::test]
async fn set_expected_hash_rejects_garbage() {
    let h = harness();
    let id = h.finished(None, "src/hello.txt");
    let e = h.pp.set_expected_hash(id, Some("1234")).unwrap_err();
    assert_eq!(e.to_string(), "Not a checksum: use 8, 32, 40 or 64 hex characters");
    h.pp.set_expected_hash(id, Some(" SHA256:D900AD473CE7307451F19C17C34DD17ABBC8035842642932863B35104A411E9C "))
        .unwrap();
    assert_eq!(
        h.store.get_download(id).unwrap().expected_hash.as_deref(),
        Some("sha256:d900ad473ce7307451f19c17c34dd17abbc8035842642932863b35104a411e9c")
    );
}

fn sha256_hex(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

async fn server_download(h: &H, corrupt: bool, bad_gets: u32, expected: &str) -> (support::TestServer, DownloadId) {
    let mut spec = Spec::new(100_000);
    spec.corrupt = corrupt;
    spec.corrupt_first_gets = bad_gets;
    let srv = serve(spec).await;
    let id = h.store.insert_download(&nd(None, &srv.url, &h.dl)).unwrap();
    h.store.set_expected_hash(id, Some(&format!("sha256:{expected}"))).unwrap();
    (srv, id)
}

#[tokio::test]
async fn mismatch_redownloads_once_then_fails() {
    let h = harness();
    let (_srv, id) = server_download(&h, true, 0, &sha256_hex(&body(100_000))).await;
    let r = h.settled().await;
    assert_eq!(r.state, RunState::Failed, "{r:#?}");
    assert_eq!(step(&r, StepKind::Verify), (StepState::Failed, "Checksum mismatch: file".into()));
    assert_eq!(h.store.get_download(id).unwrap().verify_retries, 1);
    assert_eq!(h.bin.got.lock().unwrap().len(), 1, "the first bad copy went to the Recycle Bin");
}

#[tokio::test]
async fn mismatch_then_good_passes() {
    let h = harness();
    let (_srv, id) = server_download(&h, false, 1, &sha256_hex(&body(100_000))).await;
    let r = h.settled().await;
    assert_eq!(h.store.get_download(id).unwrap().verify_retries, 1);
    assert_eq!(r.state, RunState::Done, "{r:#?}");
    assert_eq!(step(&r, StepKind::Verify).0, StepState::Done);
    assert!(h.dl.join(r"Other\file").is_file());
}

#[tokio::test]
async fn password_needed_then_given() {
    let h = harness();
    h.finish(None, "archives/secret.7z");
    let r = h.settled().await;
    assert_eq!(r.state, RunState::NeedsPassword, "{r:#?}");
    assert_eq!(step(&r, StepKind::Extract), (StepState::NeedsPassword, "Needs a password".into()));
    assert_eq!(h.notices.lock().unwrap()[0].title, "secret needs attention");
    h.pp.give_password(r.id, "haul-secret".into(), true).unwrap();
    h.wait("done", |r| r.len() == 1 && r[0].state == RunState::Done).await;
    assert_eq!(h.pp.passwords().unwrap().len(), 1);
    assert_eq!(std::fs::read_to_string(h.dl.join(r"Documents\secret\hello.txt")).unwrap(), "hello haul\n");
}

#[tokio::test]
async fn given_wrong_password_stays_needed() {
    let h = harness();
    h.finish(None, "archives/secret.rar");
    let r = h.settled().await;
    assert_eq!(r.state, RunState::NeedsPassword);
    h.pp.give_password(r.id, "nope".into(), true).unwrap();
    let r2 = h
        .wait("wrong password result", |r| {
            r.len() == 1
                && r[0].state == RunState::NeedsPassword
                && r[0].steps[1].detail.as_deref() == Some("Wrong password")
        })
        .await;
    assert_eq!(r2[0].state, RunState::NeedsPassword);
    assert!(h.pp.passwords().unwrap().is_empty());
    // Nothing half-extracted is left behind.
    assert!(!h.dl.join("secret").exists());
    assert!(!h.dl.join("secret.haul-tmp").exists());
}

#[tokio::test]
async fn saved_password_used() {
    let h = harness();
    h.pp.add_password("wrong-one").unwrap();
    h.pp.add_password("haul-secret").unwrap();
    h.finish(None, "archives/secret.rar");
    let r = h.settled().await;
    assert_eq!(r.state, RunState::Done, "{r:#?}");
    assert_eq!(h.pp.passwords().unwrap().len(), 2);
}

#[tokio::test]
async fn damaged_archive_fails() {
    let h = harness();
    h.finish(None, "archives/damaged.7z");
    let r = h.settled().await;
    assert_eq!(r.state, RunState::Failed);
    let (st, detail) = step(&r, StepKind::Extract);
    assert_eq!(st, StepState::Failed);
    assert!(detail.starts_with("Archive is damaged"), "{detail}");
    assert_eq!(step(&r, StepKind::Sort).0, StepState::Pending);
    tokio::time::sleep(Duration::from_millis(1100)).await; // updated_at has 1 s resolution
    h.pp.retry(r.id).unwrap();
    let r2 = h.settled_again(&r.updated_at).await;
    assert_eq!(r2.state, RunState::Failed);
    assert!(r2.updated_at > r.updated_at);
    assert_eq!(h.notices.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn nested_extracted_one_level() {
    let h = harness();
    h.finish(None, "archives/nested.zip");
    let r = h.settled().await;
    assert_eq!(r.state, RunState::Done, "{r:#?}");
    // nested.zip -> nested/plain.7z, and plain.7z -> nested/plain/; the folder's largest file is plain.7z.
    let out = h.dl.join(r"Compressed\nested");
    assert!(out.join("plain.7z").is_file(), "inner archives are kept");
    assert_eq!(std::fs::read_to_string(out.join(r"plain\hello.txt")).unwrap(), "hello haul\n");
    assert!(!out.join(r"plain\plain").exists(), "only one level deep");
}

#[tokio::test]
async fn auto_off_holds_until_process() {
    let h = harness();
    h.pp.set_settings(PostProcSettings { auto: false, ..Default::default() }).unwrap();
    let id = h.finish(None, "archives/plain.zip");
    let r = h.settled().await;
    assert_eq!(r.state, RunState::Held);
    assert!(h.dl.join("plain.zip").is_file());
    h.pp.process(None, Some(id)).unwrap();
    let r = h.wait("done", |r| r.len() == 1 && r[0].state == RunState::Done).await;
    assert_eq!(r[0].download_id, Some(id));
}

#[tokio::test]
async fn process_refuses_unfinished() {
    let h = harness();
    let id = h.store.insert_download(&nd(None, "https://x.org/a.bin", &h.dl)).unwrap();
    assert_eq!(h.pp.process(None, Some(id)).unwrap_err().to_string(), "Not finished yet");
}

#[tokio::test]
async fn recycle_archives_when_on() {
    let h = harness();
    h.pp.set_settings(PostProcSettings { recycle_archives: true, ..Default::default() }).unwrap();
    h.finish(None, "archives/plain.zip");
    let r = h.settled().await;
    assert_eq!(r.state, RunState::Done, "{r:#?}");
    assert_eq!(h.bin.got.lock().unwrap().clone(), vec![h.dl.join("plain.zip")]);
    assert!(h.dl.join(r"Documents\plain\hello.txt").is_file());
    assert!(!h.dl.join(r"Compressed\plain.zip").exists());
}

#[tokio::test]
async fn rule_beats_category() {
    let h = harness();
    h.pp.set_rules(vec![Rule {
        id: 0,
        name: "Texts".into(),
        enabled: true,
        matcher: RuleMatch { name: Some("*.txt".into()), ..Default::default() },
        action: RuleAction { folder: Some("Texts".into()), rename: Some("{host}-{name}.{ext}".into()) },
    }])
    .unwrap();
    let id = h.finish(None, "src/hello.txt");
    let r = h.settled().await;
    let want = h.dl.join(r"Texts\files.example.org-hello.txt");
    assert!(want.is_file(), "{r:#?}");
    assert_eq!(step(&r, StepKind::Sort).1, format!("Rule “Texts” → {}", h.dl.join("Texts").display()));
    assert_eq!(h.store.get_download(id).unwrap().target_path, Some(want));
}

#[tokio::test]
async fn set_rules_rejects_bad_regex_and_keeps_old() {
    let h = harness();
    let good = Rule {
        id: 0,
        name: "Good".into(),
        enabled: true,
        matcher: RuleMatch::default(),
        action: RuleAction { folder: Some("G".into()), rename: None },
    };
    h.pp.set_rules(vec![good.clone()]).unwrap();
    let mut bad = good.clone();
    bad.name = "Bad".into();
    bad.matcher.name = Some("/(/".into());
    assert!(h.pp.set_rules(vec![bad]).unwrap_err().to_string().starts_with("Bad pattern in “Bad”"));
    let rules = h.pp.rules().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "Good");
}

#[tokio::test]
async fn missing_file_fails_step() {
    let h = harness();
    let id = h.finished(None, "src/hello.txt");
    let path = h.dl.join("hello.txt");
    std::fs::rename(&path, h.dl.join("aside.txt")).unwrap();
    h.complete(id);
    let r = h.settled().await;
    assert_eq!(r.state, RunState::Failed);
    assert_eq!(step(&r, StepKind::Sort), (StepState::Failed, "File missing: hello.txt".into()));
    std::fs::rename(h.dl.join("aside.txt"), &path).unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    h.pp.retry(r.id).unwrap();
    let r = h.settled_again(&r.updated_at).await;
    assert_eq!(r.state, RunState::Done, "{r:#?}");
    assert!(h.dl.join(r"Documents\hello.txt").is_file());
}

#[tokio::test]
async fn interrupted_extract_restarts_clean() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let dl = tmp.path().join("dl");
    std::fs::create_dir_all(dl.join("plain.haul-tmp")).unwrap();
    std::fs::create_dir_all(dl.join("plain")).unwrap();
    std::fs::write(dl.join(r"plain.haul-tmp\half.txt"), "half").unwrap();
    std::fs::copy(fixture("archives/plain.zip"), dl.join("plain.zip")).unwrap();
    let id = store.insert_download(&nd(None, "https://files.example.org/plain.zip", &dl)).unwrap();
    store.set_completed(id, &dl.join("plain.zip")).unwrap();
    let out = dl.join("plain").to_string_lossy().replace('\\', "\\\\");
    let steps = format!(
        r#"{{"steps":[{{"kind":"verify","state":"skipped","detail":"No checksum"}},{{"kind":"extract","state":"running","detail":null}},{{"kind":"sort","state":"pending","detail":null}},{{"kind":"scan","state":"pending","detail":null}},{{"kind":"notify","state":"pending","detail":null}}],"extractedTo":"{out}"}}"#
    );
    store.ensure_run(None, Some(id), "running", &steps).unwrap();
    let h = harness_with(store, tmp);
    let r = h.wait("done", |r| r.len() == 1 && r[0].state == RunState::Done).await;
    assert_eq!(r[0].state, RunState::Done);
    assert!(h.dl.join(r"Documents\plain\hello.txt").is_file(), "{r:#?}");
    assert!(!h.dl.join(r"Documents\plain\half.txt").exists());
    assert!(!h.dl.join(r"Documents\plain (1)").exists());
    assert!(!h.dl.join("plain (1)").exists());
    assert!(!h.dl.join("plain.haul-tmp").exists());
}

#[tokio::test]
async fn lone_download_gets_its_own_run() {
    let h = harness();
    let id = h.finish(None, "src/hello.txt");
    let r = h.settled().await;
    assert_eq!((r.package_id, r.download_id), (None, Some(id)));
    assert_eq!(h.notices.lock().unwrap()[0].title, "hello is ready");
}

#[tokio::test]
async fn package_with_unfinished_file_waits() {
    let h = harness();
    let pkg = h.store.create_package("two", "paste").unwrap();
    let a = h.finished(Some(pkg), "src/hello.txt");
    let b = h.store.insert_download(&nd(Some(pkg), "https://x.org/b.bin", &h.dl)).unwrap();
    h.store.set_state(b, DownloadState::Paused, None).unwrap();
    h.complete(a);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(h.pp.runs().unwrap().is_empty());
}
