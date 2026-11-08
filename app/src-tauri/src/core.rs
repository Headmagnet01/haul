//! App commands as plain methods on `AppCore`, so they test without Tauri.

use crate::dto::{
    Category, CollectorFilters, CollectorSnapshot, Destination, DownloadView, IntakeResult, PackageView,
    PostProcSettings, Rule, RulesConfig, Run, Snapshot, SpeedLimit, StartResult,
};
use haul_engine::postproc::extract::SevenZip;
use haul_engine::postproc::scan::Defender;
use haul_engine::postproc::sort::RecycleBin;
use haul_engine::postproc::{Notice, PostProc, PostProcDeps};
use haul_engine::{
    Download, DownloadState, EngineError, EventBus, Grabber, HostLimiter, Priority, ProbeFn, Queue, RateLimiter,
    Schedule, StartDefaults, Store, TransferConfig, TransferCtx, extract_links, http_prober, naming,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const KEY_SPEED_LIMIT: &str = "speed_limit";
pub const KEY_DOWNLOAD_DIR: &str = "download_dir";
pub const KEY_MAX_PARALLEL: &str = "max_parallel";
pub const KEY_WATCH_CLIPBOARD: &str = "watch_clipboard";
pub const KEY_COLLECTOR_FILTERS: &str = "collector_filters";
/// Where a link may come from.
const SOURCES: [&str; 4] = ["clipboard", "paste", "drop", "browser"];
const DEFAULT_MAX_PARALLEL: usize = 3;
pub(crate) const DEFAULT_CONNECTIONS: u32 = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FolderTarget {
    RevealFile(PathBuf),
    OpenDir(PathBuf),
}

/// Cheap to clone: every field is a shared handle, so commands can move a copy onto a blocking thread.
#[derive(Clone)]
pub struct AppCore {
    pub(crate) store: Store,
    ctx: TransferCtx,
    pub(crate) queue: Queue,
    pub(crate) grabber: Grabber,
    clipboard_on: Arc<AtomicBool>,
    pub(crate) download_dir: PathBuf,
    postproc: PostProc,
    /// Browser captures in the last few seconds, by URL: (download id, when).
    pub(crate) recent: Arc<Mutex<HashMap<String, (i64, Instant)>>>,
}

fn priority_str(p: Priority) -> &'static str {
    match p {
        Priority::High => "high",
        Priority::Normal => "normal",
        Priority::Low => "low",
    }
}

fn invalid(msg: &str) -> EngineError {
    EngineError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, msg.to_string()))
}

fn json<T: serde::Serialize>(v: &T) -> Result<serde_json::Value, EngineError> {
    serde_json::to_value(v).map_err(|e| EngineError::Io(std::io::Error::other(e)))
}

/// Lowercase, no leading dot, no blanks or repeats.
fn normalise_exts(exts: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in exts {
        let e = e.trim().trim_start_matches('.').to_lowercase();
        if !e.is_empty() && !out.contains(&e) {
            out.push(e);
        }
    }
    out
}

fn parse_priority(s: &str) -> Result<Priority, EngineError> {
    match s {
        "high" => Ok(Priority::High),
        "normal" => Ok(Priority::Normal),
        "low" => Ok(Priority::Low),
        _ => Err(invalid("unknown priority")),
    }
}

fn display_name(d: &Download) -> String {
    // A finished file may have been saved as "name (1).ext"; show what is on disk.
    if d.state == DownloadState::Completed
        && let Some(n) = d.target_path.as_deref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy())
    {
        return n.into_owned();
    }
    if let Some(n) = d.file_name.as_deref().filter(|n| !n.is_empty()) {
        return n.to_string();
    }
    if let Some(n) = d.probe.as_ref().map(|p| p.file_name.as_str()).filter(|n| !n.is_empty()) {
        return n.to_string();
    }
    match url::Url::parse(&d.url) {
        Ok(u) => naming::filename_from(None, &u),
        Err(_) => d.url.clone(),
    }
}

impl AppCore {
    /// Opens (or creates) the database at `db_path`. Must run inside a Tokio runtime.
    pub fn start(db_path: &Path, default_download_dir: PathBuf, deps: PostProcDeps) -> Result<AppCore, EngineError> {
        let prober = http_prober(haul_engine::http_client());
        Ok(Self::build(Store::open(db_path)?, default_download_dir, None, prober, deps))
    }

    /// The real post-processing parts: the bundled 7-Zip, the Recycle Bin, Windows Defender.
    pub fn real_deps(seven_zip: PathBuf, notify: impl Fn(Notice) + Send + Sync + 'static) -> PostProcDeps {
        PostProcDeps {
            extractor: Arc::new(SevenZip { exe: seven_zip }),
            recycler: Arc::new(RecycleBin),
            scanner: Arc::new(Defender::default()),
            notify: Arc::new(notify),
        }
    }

    /// For tests: the 7-Zip in the source tree and no notifications.
    fn quiet_deps() -> PostProcDeps {
        Self::real_deps(haul_engine::postproc::extract::seven_zip_for_tests(), |_| {})
    }

    /// `schedule` lets tests keep the queue from ever starting a download.
    pub fn start_with(store: Store, default_download_dir: PathBuf, schedule: Option<Schedule>) -> AppCore {
        let prober = http_prober(haul_engine::http_client());
        Self::start_full(store, default_download_dir, schedule, prober)
    }

    /// Like `start_with`, with the Collector's link checker injected (tests check links without a network).
    pub fn start_full(
        store: Store,
        default_download_dir: PathBuf,
        schedule: Option<Schedule>,
        prober: ProbeFn,
    ) -> AppCore {
        Self::build(store, default_download_dir, schedule, prober, Self::quiet_deps())
    }

    fn build(
        store: Store,
        default_download_dir: PathBuf,
        schedule: Option<Schedule>,
        prober: ProbeFn,
        deps: PostProcDeps,
    ) -> AppCore {
        let limit: SpeedLimit = store
            .get_setting(KEY_SPEED_LIMIT)
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let download_dir = store
            .get_setting(KEY_DOWNLOAD_DIR)
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(PathBuf::from))
            .unwrap_or(default_download_dir);
        let max_parallel = store
            .get_setting(KEY_MAX_PARALLEL)
            .ok()
            .flatten()
            .and_then(|v| v.as_u64())
            .map_or(DEFAULT_MAX_PARALLEL, |n| n as usize);
        let ctx = TransferCtx {
            client: haul_engine::http_client(),
            store: store.clone(),
            events: EventBus::new(),
            global_limit: Arc::new(RateLimiter::new(limit.enabled.then_some(limit.bps))),
            hosts: Arc::new(HostLimiter::new(16)),
            config: TransferConfig::default(),
        };
        let queue = Queue::start(ctx.clone(), max_parallel);
        if schedule.is_some() {
            queue.set_schedule(schedule);
        }
        let grabber = Grabber::new(store.clone(), ctx.events.clone(), prober);
        let watch = store.get_setting(KEY_WATCH_CLIPBOARD).ok().flatten().and_then(|v| v.as_bool()).unwrap_or(false);
        let postproc = PostProc::start(store.clone(), ctx.events.clone(), queue.clone(), deps, download_dir.clone());
        AppCore {
            store,
            ctx,
            queue,
            grabber,
            clipboard_on: Arc::new(AtomicBool::new(watch)),
            download_dir,
            postproc,
            recent: Arc::default(),
        }
    }

    pub fn events(&self) -> EventBus {
        self.ctx.events.clone()
    }

    fn speed_limit(&self) -> SpeedLimit {
        self.store
            .get_setting(KEY_SPEED_LIMIT)
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    fn view(&self, d: Download, segs: &[haul_engine::Range]) -> DownloadView {
        let size = d.probe.as_ref().and_then(|p| p.size);
        let sum: u64 = segs.iter().map(|r| r.pos.saturating_sub(r.start)).sum();
        let written = if d.state == DownloadState::Completed { size.unwrap_or(sum) } else { sum };
        DownloadView {
            id: d.id,
            package_id: d.package_id,
            name: display_name(&d),
            host: url::Url::parse(&d.url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default(),
            url: d.url,
            size,
            written,
            state: d.state.as_str().to_string(),
            error: d.error,
            priority: priority_str(d.priority).to_string(),
            position: d.position,
            connections: d.connections,
            resumable: d.probe.as_ref().map(|p| p.resumable),
            target_dir: d.target_dir.to_string_lossy().into_owned(),
            target_path: d.target_path.map(|p| p.to_string_lossy().into_owned()),
            created_at: d.created_at,
            segments: segs.iter().map(|r| [r.start, r.pos, r.end]).collect(),
        }
    }

    pub fn snapshot(&self) -> Result<Snapshot, EngineError> {
        let mut segs = self.store.load_all_segments()?;
        let downloads: Vec<DownloadView> = self
            .store
            .list_downloads()?
            .into_iter()
            .map(|d| {
                let s = segs.remove(&d.id).unwrap_or_default();
                self.view(d, &s)
            })
            .collect();
        let used: HashSet<i64> = downloads.iter().filter_map(|d| d.package_id).collect();
        let packages = self
            .store
            .list_packages()?
            .into_iter()
            .filter(|p| used.contains(&p.id))
            .map(|p| PackageView { id: p.id, name: p.name, created_at: p.created_at })
            .collect();
        Ok(Snapshot { packages, downloads, speed_limit: self.speed_limit() })
    }

    /// Sends every link in `text` to the Collector.
    pub fn add_links(&self, text: &str, source: &str) -> Result<IntakeResult, EngineError> {
        if !SOURCES.contains(&source) {
            return Err(invalid("unknown link source"));
        }
        let links = extract_links(text);
        if links.is_empty() {
            return Ok(IntakeResult { added: 0, skipped: 0 });
        }
        self.grabber.intake(links, source)
    }

    fn collector_filters(&self) -> CollectorFilters {
        self.store
            .get_setting(KEY_COLLECTOR_FILTERS)
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    pub fn collector_snapshot(&self) -> Result<CollectorSnapshot, EngineError> {
        let s = self.grabber.state()?;
        Ok(CollectorSnapshot {
            packages: s.packages,
            links: s.links,
            filters: self.collector_filters(),
            watch_clipboard: self.watch_clipboard(),
        })
    }

    pub fn collector_start(&self, ids: &[i64]) -> Result<StartResult, EngineError> {
        let d = StartDefaults { target_dir: self.download_dir.clone(), connections: DEFAULT_CONNECTIONS };
        Ok(StartResult { started: self.grabber.start(ids, &self.queue, &d)? })
    }

    pub fn collector_remove(&self, ids: &[i64]) -> Result<(), EngineError> {
        self.grabber.remove(ids)
    }

    pub fn collector_recheck(&self, ids: &[i64]) -> Result<(), EngineError> {
        self.grabber.recheck(ids)
    }

    pub fn set_collector_filters(&self, f: CollectorFilters) -> Result<(), EngineError> {
        let f = CollectorFilters { exts: normalise_exts(&f.exts), ..f };
        self.store.set_setting(KEY_COLLECTOR_FILTERS, &json(&f)?)
    }

    pub fn set_watch_clipboard(&self, on: bool) -> Result<(), EngineError> {
        self.store.set_setting(KEY_WATCH_CLIPBOARD, &serde_json::Value::Bool(on))?;
        self.clipboard_on.store(on, Ordering::Relaxed);
        Ok(())
    }

    pub fn watch_clipboard(&self) -> bool {
        self.clipboard_on.load(Ordering::Relaxed)
    }

    pub fn pause(&self, ids: &[i64]) {
        ids.iter().for_each(|&id| self.queue.pause(id));
    }

    pub fn resume(&self, ids: &[i64]) {
        ids.iter().for_each(|&id| self.queue.resume(id));
    }

    pub fn remove(&self, ids: &[i64]) {
        ids.iter().for_each(|&id| self.queue.remove(id));
    }

    pub fn pause_all(&self) {
        if let Ok(ds) = self.store.list_downloads() {
            ds.iter()
                .filter(|d| matches!(d.state, DownloadState::Queued | DownloadState::Running))
                .for_each(|d| self.queue.pause(d.id));
        }
    }

    pub fn set_priority(&self, id: i64, priority: &str) -> Result<(), EngineError> {
        self.queue.set_priority(id, parse_priority(priority)?);
        Ok(())
    }

    pub fn reorder(&self, ids: &[i64]) {
        self.queue.reorder(ids);
    }

    pub fn set_speed_limit(&self, limit: SpeedLimit) -> Result<(), EngineError> {
        let v = serde_json::to_value(limit).map_err(|e| EngineError::Io(std::io::Error::other(e)))?;
        self.store.set_setting(KEY_SPEED_LIMIT, &v)?;
        self.queue.set_global_limit(limit.enabled.then_some(limit.bps));
        Ok(())
    }

    pub fn folder_target(&self, id: i64) -> Result<FolderTarget, EngineError> {
        let d = self.store.get_download(id)?;
        Ok(match d.target_path {
            Some(p) if d.state == DownloadState::Completed => FolderTarget::RevealFile(p),
            _ => FolderTarget::OpenDir(d.target_dir),
        })
    }

    pub fn postproc_runs(&self) -> Result<Vec<Run>, EngineError> {
        self.postproc.runs()
    }

    pub fn postproc_process(&self, package_id: Option<i64>, download_id: Option<i64>) -> Result<i64, EngineError> {
        self.postproc.process(package_id, download_id)
    }

    pub fn postproc_retry(&self, run_id: i64) -> Result<(), EngineError> {
        self.postproc.retry(run_id)
    }

    pub fn postproc_password(&self, run_id: i64, password: String, remember: bool) -> Result<(), EngineError> {
        self.postproc.give_password(run_id, password, remember)
    }

    pub fn set_expected_hash(&self, id: i64, hash: Option<&str>) -> Result<(), EngineError> {
        self.postproc.set_expected_hash(id, hash)
    }

    pub fn rules_config(&self) -> Result<RulesConfig, EngineError> {
        Ok(RulesConfig {
            settings: self.postproc.settings(),
            categories: self.postproc.categories()?,
            rules: self.postproc.rules()?,
            passwords: self.postproc.passwords()?,
        })
    }

    pub fn set_postproc_settings(&self, s: PostProcSettings) -> Result<(), EngineError> {
        self.postproc.set_settings(s)
    }

    pub fn set_categories(&self, c: Vec<Category>) -> Result<(), EngineError> {
        self.postproc.set_categories(c)
    }

    pub fn set_rules(&self, r: Vec<Rule>) -> Result<(), EngineError> {
        self.postproc.set_rules(r)
    }

    /// Where a file would go under a draft of the rules (nothing is saved).
    pub fn try_rules(
        &self,
        name: &str,
        host: &str,
        size: Option<u64>,
        rules: &[Rule],
        categories: &[Category],
        sort: bool,
    ) -> Result<Destination, EngineError> {
        let facts =
            haul_engine::rules::Facts { name, host, size, package: "Package", date: chrono::Local::now().date_naive() };
        haul_engine::postproc::try_rules(&facts, rules, categories, sort, &self.download_dir)
            .map_err(EngineError::Invalid)
    }

    pub fn add_password(&self, pw: &str) -> Result<i64, EngineError> {
        self.postproc.add_password(pw)
    }

    pub fn remove_password(&self, id: i64) -> Result<(), EngineError> {
        self.postproc.remove_password(id)
    }

    pub fn order_passwords(&self, ids: &[i64]) -> Result<(), EngineError> {
        self.postproc.order_passwords(ids)
    }

    /// Pauses everything and saves; used on exit.
    pub async fn shutdown(&self) {
        self.queue.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::SpeedLimit;
    use chrono::NaiveTime;
    use haul_engine::{Schedule, Store};

    fn never() -> Option<Schedule> {
        let t = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        Some(Schedule { start: t, stop: t, weekdays: vec![] })
    }

    fn core(store: Store, dir: &std::path::Path) -> AppCore {
        AppCore::start_with(store, dir.to_path_buf(), never())
    }

    /// Queues `https://a.org/f.bin` without the Collector.
    fn queue_one(c: &AppCore) {
        c.queue
            .add(haul_engine::NewDownload {
                package_id: None,
                url: "https://a.org/f.bin".into(),
                headers_json: None,
                target_dir: c.download_dir.clone(),
                file_name: None,
                priority: Priority::Normal,
                connections: DEFAULT_CONNECTIONS,
                speed_limit: None,
            })
            .unwrap();
    }

    fn fake_online() -> haul_engine::ProbeFn {
        std::sync::Arc::new(|url: String| {
            Box::pin(async move {
                Ok(haul_engine::ProbeInfo {
                    final_url: url.clone(),
                    size: Some(1_000_000),
                    resumable: true,
                    file_name: url.rsplit('/').next().unwrap_or_default().to_string(),
                    etag: None,
                    last_modified: None,
                    content_type: None,
                    digest: None,
                })
            })
        })
    }

    fn core_fake(store: Store, dir: &std::path::Path) -> AppCore {
        AppCore::start_full(store, dir.to_path_buf(), never(), fake_online())
    }

    async fn wait_checked(c: &AppCore) -> crate::dto::CollectorSnapshot {
        for _ in 0..300 {
            let s = c.collector_snapshot().unwrap();
            if !s.links.is_empty() && s.links.iter().all(|l| l.status != haul_engine::LinkStatus::Checking) {
                return s;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("links never checked");
    }

    #[tokio::test]
    async fn add_links_goes_to_collector() {
        let dir = tempfile::tempdir().unwrap();
        let c = core_fake(Store::open_in_memory().unwrap(), dir.path());
        let r = c.add_links("x https://a.org/1.bin y https://a.org/2.bin", "paste").unwrap();
        assert_eq!(r, IntakeResult { added: 2, skipped: 0 });
        assert!(c.snapshot().unwrap().downloads.is_empty());
        let s = c.collector_snapshot().unwrap();
        assert_eq!(s.links.len(), 2);
        assert!(s.links.iter().all(|l| l.source == "paste"));
    }

    #[tokio::test]
    async fn add_links_rejects_unknown_source() {
        let dir = tempfile::tempdir().unwrap();
        let c = core_fake(Store::open_in_memory().unwrap(), dir.path());
        assert!(c.add_links("https://a.org/x", "fax").is_err());
        assert!(c.collector_snapshot().unwrap().links.is_empty());
    }

    #[tokio::test]
    async fn add_no_links() {
        let dir = tempfile::tempdir().unwrap();
        let c = core_fake(Store::open_in_memory().unwrap(), dir.path());
        let r = c.add_links("nothing here", "paste").unwrap();
        assert_eq!(r, IntakeResult { added: 0, skipped: 0 });
        assert!(c.store.list_batches().unwrap().is_empty());
    }

    #[tokio::test]
    async fn collector_start_moves_to_queue() {
        let dir = tempfile::tempdir().unwrap();
        let c = core_fake(Store::open_in_memory().unwrap(), dir.path());
        c.add_links("https://a.org/1.bin https://a.org/2.bin", "drop").unwrap();
        let s = wait_checked(&c).await;
        let ids: Vec<i64> = s.links.iter().map(|l| l.id).collect();
        assert_eq!(c.collector_start(&ids).unwrap(), StartResult { started: 2 });
        let q = c.snapshot().unwrap();
        assert_eq!(q.downloads.len(), 2);
        assert!(q.downloads[0].package_id.is_some() && q.downloads[0].package_id == q.downloads[1].package_id);
        assert_eq!(q.downloads[0].size, Some(1_000_000));
        assert_eq!(q.downloads[0].target_dir, dir.path().to_string_lossy());
        assert!(c.collector_snapshot().unwrap().links.is_empty());
        c.shutdown().await;
    }

    #[tokio::test]
    async fn filters_default_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("haul.db");
        {
            let c = core_fake(Store::open(&db).unwrap(), dir.path());
            let f = c.collector_snapshot().unwrap().filters;
            assert_eq!(f, CollectorFilters { html: true, images: true, small: false, duplicates: false, exts: vec![] });
            c.set_collector_filters(CollectorFilters {
                html: false,
                images: true,
                small: true,
                duplicates: true,
                exts: vec![".ISO".into(), "iso".into(), "".into(), " .Mkv ".into()],
            })
            .unwrap();
            c.shutdown().await;
        }
        let c = core_fake(Store::open(&db).unwrap(), dir.path());
        assert_eq!(
            c.collector_snapshot().unwrap().filters,
            CollectorFilters {
                html: false,
                images: true,
                small: true,
                duplicates: true,
                exts: vec!["iso".into(), "mkv".into()]
            }
        );
    }

    #[tokio::test]
    async fn watch_clipboard_defaults_off_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let c = core_fake(store.clone(), dir.path());
        assert!(!c.watch_clipboard());
        assert!(!c.collector_snapshot().unwrap().watch_clipboard);
        c.set_watch_clipboard(true).unwrap();
        assert!(c.watch_clipboard());
        c.shutdown().await;
        let fresh = core_fake(store, dir.path());
        assert!(fresh.watch_clipboard());
        assert!(fresh.collector_snapshot().unwrap().watch_clipboard);
    }

    #[tokio::test]
    async fn rules_config_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        let r = c.rules_config().unwrap();
        assert_eq!(r.settings, haul_engine::postproc::PostProcSettings::default());
        assert!(
            r.settings.auto && r.settings.sort_categories && !r.settings.recycle_archives && !r.settings.virus_scan
        );
        assert_eq!(r.categories.len(), 6);
        assert!(r.rules.is_empty());
        assert!(r.passwords.is_empty());
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["settings"]["sortCategories"], true);
        assert!(v["categories"][0]["exts"].is_array());
    }

    fn rule(name: &str, pattern: &str, folder: &str) -> haul_engine::rules::Rule {
        haul_engine::rules::Rule {
            id: 0,
            name: name.into(),
            enabled: true,
            matcher: haul_engine::rules::RuleMatch { name: Some(pattern.into()), ..Default::default() },
            action: haul_engine::rules::RuleAction { folder: Some(folder.into()), rename: None },
        }
    }

    #[tokio::test]
    async fn set_rules_rejects_bad_regex_and_keeps_old() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        c.set_rules(vec![rule("ISO", "*.iso", r"Programs\ISO")]).unwrap();
        let e = c.set_rules(vec![rule("Bad", "/(/", "x")]).unwrap_err();
        assert!(e.to_string().starts_with("Bad pattern in “Bad”"), "{e}");
        let rules = c.rules_config().unwrap().rules;
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "ISO");
    }

    #[tokio::test]
    async fn try_rules_reports_rule() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        let cats = haul_engine::rules::default_categories();
        let d = c
            .try_rules(
                "ubuntu.iso",
                "releases.ubuntu.com",
                Some(5),
                &[rule("ISO", "*.iso", r"Programs\ISO")],
                &cats,
                true,
            )
            .unwrap();
        assert_eq!(d.by, "Rule “ISO”");
        assert_eq!(d.folder, Some(dir.path().join(r"Programs\ISO")));
        let d = c.try_rules("film.mkv", "", None, &[], &cats, true).unwrap();
        assert_eq!((d.by.as_str(), d.folder), ("Video", Some(dir.path().join("Video"))));
        assert!(c.try_rules("a", "", None, &[rule("Bad", "/(/", "x")], &cats, true).is_err());
    }

    #[tokio::test]
    async fn password_roundtrip_hides_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        let id = c.add_password("hunter2-secret").unwrap();
        let r = c.rules_config().unwrap();
        assert_eq!(r.passwords, vec![id]);
        assert!(!serde_json::to_string(&r).unwrap().contains("hunter2-secret"));
        c.remove_password(id).unwrap();
        assert!(c.rules_config().unwrap().passwords.is_empty());
    }

    #[tokio::test]
    async fn postproc_process_requires_finished() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        queue_one(&c);
        let id = c.snapshot().unwrap().downloads[0].id;
        assert_eq!(c.postproc_process(None, Some(id)).unwrap_err().to_string(), "Not finished yet");
        assert!(c.postproc_runs().unwrap().is_empty());
    }

    #[tokio::test]
    async fn postproc_settings_persist() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        let s = haul_engine::postproc::PostProcSettings { virus_scan: true, ..Default::default() };
        c.set_postproc_settings(s).unwrap();
        assert!(c.rules_config().unwrap().settings.virus_scan);
    }

    #[tokio::test]
    async fn speed_limit_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let c = core(store.clone(), dir.path());
        assert_eq!(c.snapshot().unwrap().speed_limit, SpeedLimit { enabled: false, bps: 10_000_000 });
        c.set_speed_limit(SpeedLimit { enabled: true, bps: 5_000_000 }).unwrap();
        assert_eq!(c.ctx.global_limit.rate(), Some(5_000_000));
        c.shutdown().await;
        let fresh = core(store, dir.path());
        assert_eq!(fresh.snapshot().unwrap().speed_limit, SpeedLimit { enabled: true, bps: 5_000_000 });
        assert_eq!(fresh.ctx.global_limit.rate(), Some(5_000_000));
    }

    #[tokio::test]
    async fn set_priority_rejects_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        queue_one(&c);
        let id = c.snapshot().unwrap().downloads[0].id;
        assert!(c.set_priority(id, "urgent").is_err());
        c.set_priority(id, "high").unwrap();
        assert_eq!(c.snapshot().unwrap().downloads[0].priority, "high");
    }

    #[tokio::test]
    async fn completed_row_shows_saved_name() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let c = core(store.clone(), dir.path());
        queue_one(&c);
        let id = c.snapshot().unwrap().downloads[0].id;
        let saved = dir.path().join("f (1).bin");
        store.set_completed(id, &saved).unwrap();
        let d = &c.snapshot().unwrap().downloads[0];
        assert_eq!(d.name, "f (1).bin");
        assert_eq!(c.folder_target(id).unwrap(), FolderTarget::RevealFile(saved));
    }

    #[tokio::test]
    async fn folder_target_for_unfinished_is_dir() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        queue_one(&c);
        let id = c.snapshot().unwrap().downloads[0].id;
        assert_eq!(c.folder_target(id).unwrap(), FolderTarget::OpenDir(dir.path().to_path_buf()));
    }
}
