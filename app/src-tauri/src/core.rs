//! App commands as plain methods on `AppCore`, so they test without Tauri.

use crate::dto::{
    Category, CollectorFilters, CollectorSnapshot, Destination, DownloadView, IntakeResult, PackageView,
    PostProcSettings, Rule, RulesConfig, Run, Snapshot, SpeedLimit, StartResult,
};
use crate::settings::{Settings, SettingsPatch};
use haul_engine::postproc::extract::SevenZip;
use haul_engine::postproc::scan::Defender;
use haul_engine::postproc::sort::{RecycleBin, Recycler};
use haul_engine::postproc::{Notice, PostProc, PostProcDeps};
use haul_engine::{
    Download, DownloadState, EngineError, EventBus, Grabber, HostLimiter, Priority, ProbeFn, Queue, RateLimiter,
    Schedule, StartDefaults, Store, TransferConfig, TransferCtx, extract_links, http_prober, naming,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

pub const KEY_COLLECTOR_FILTERS: &str = "collector_filters";
/// Where a link may come from.
const SOURCES: [&str; 4] = ["clipboard", "paste", "drop", "browser"];

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
    settings: Arc<RwLock<Settings>>,
    postproc: PostProc,
    /// Finished view's Delete file…; the same Recycle Bin post-processing uses.
    recycler: Arc<dyn Recycler>,
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

/// Windows' "being used by another process" (`ERROR_SHARING_VIOLATION`, `ERROR_LOCK_VIOLATION`).
fn in_use(e: &str) -> bool {
    e.contains("os error 32") || e.contains("os error 33") || e.contains("being used by another process")
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
        let has_downloads = store.list_downloads().is_ok_and(|d| !d.is_empty());
        let settings = Settings::load(&store, default_download_dir, has_downloads);
        let limit = settings.speed_limit;
        let ctx = TransferCtx {
            client: haul_engine::http_client(),
            store: store.clone(),
            events: EventBus::new(),
            global_limit: Arc::new(RateLimiter::new(limit.enabled.then_some(limit.bps))),
            hosts: Arc::new(HostLimiter::new(16)),
            config: TransferConfig::default(),
        };
        let queue = Queue::start(ctx.clone(), settings.max_parallel);
        // Tests pass a schedule that never opens; otherwise the saved one applies.
        let schedule = schedule.or_else(|| settings.schedule.clone());
        if schedule.is_some() {
            queue.set_schedule(schedule);
        }
        let grabber = Grabber::new(store.clone(), ctx.events.clone(), prober);
        let recycler = deps.recycler.clone();
        let postproc =
            PostProc::start(store.clone(), ctx.events.clone(), queue.clone(), deps, settings.download_dir.clone());
        AppCore {
            store,
            ctx,
            queue,
            grabber,
            settings: Arc::new(RwLock::new(settings)),
            postproc,
            recycler,
            recent: Arc::default(),
        }
    }

    pub fn events(&self) -> EventBus {
        self.ctx.events.clone()
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Validates, saves and applies `patch`; returns the whole record. Nothing is saved if any field is invalid.
    /// Autostart is applied by the command layer, which has the app handle.
    pub fn set_settings(&self, patch: SettingsPatch) -> Result<Settings, EngineError> {
        let patch = crate::settings::validate(patch).map_err(|m| invalid(&m))?;
        let mut guard = self.settings.write().unwrap_or_else(|e| e.into_inner());
        let mut next = guard.clone();
        next.apply(&patch);
        next.save(&self.store, &patch)?;
        *guard = next.clone();
        drop(guard);
        if patch.max_parallel.is_some() {
            self.queue.set_max_parallel(next.max_parallel);
        }
        if patch.speed_limit.is_some() {
            self.queue.set_global_limit(next.speed_limit.enabled.then_some(next.speed_limit.bps));
        }
        if patch.schedule.is_some() {
            self.queue.set_schedule(next.schedule.clone());
        }
        if patch.download_dir.is_some() {
            self.postproc.set_download_dir(next.download_dir.clone());
        }
        Ok(next)
    }

    /// Where new downloads go.
    pub(crate) fn download_dir(&self) -> PathBuf {
        self.settings.read().unwrap_or_else(|e| e.into_inner()).download_dir.clone()
    }

    /// Connections for a new download.
    pub(crate) fn connections(&self) -> u32 {
        self.settings.read().unwrap_or_else(|e| e.into_inner()).connections
    }

    fn speed_limit(&self) -> SpeedLimit {
        self.settings.read().unwrap_or_else(|e| e.into_inner()).speed_limit
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
            finished_at: d.finished_at,
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
        let d = StartDefaults { target_dir: self.download_dir(), connections: self.connections() };
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

    pub fn watch_clipboard(&self) -> bool {
        self.settings.read().unwrap_or_else(|e| e.into_inner()).watch_clipboard
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

    /// Queues every paused download again (the tray's Resume all).
    pub fn resume_all(&self) {
        if let Ok(ds) = self.store.list_downloads() {
            ds.iter().filter(|d| d.state == DownloadState::Paused).for_each(|d| self.queue.resume(d.id));
        }
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

    pub fn folder_target(&self, id: i64) -> Result<FolderTarget, EngineError> {
        let d = self.store.get_download(id)?;
        Ok(match d.target_path {
            Some(p) if d.state == DownloadState::Completed => FolderTarget::RevealFile(p),
            _ => FolderTarget::OpenDir(d.target_dir),
        })
    }

    /// A completed download's saved file, if it is still on disk.
    pub fn finished_file(&self, id: i64) -> Result<PathBuf, EngineError> {
        let d = self.store.get_download(id)?;
        match d.target_path {
            Some(p) if d.state == DownloadState::Completed && p.exists() => Ok(p),
            _ => Err(invalid("That file is no longer on disk.")),
        }
    }

    /// The ids among `ids` whose saved file exists.
    pub fn files_exist(&self, ids: &[i64]) -> Vec<i64> {
        ids.iter()
            .copied()
            .filter(|&id| self.store.get_download(id).ok().and_then(|d| d.target_path).is_some_and(|p| p.exists()))
            .collect()
    }

    /// Sends each saved file to the Recycle Bin, then deletes its row. A file already gone is not an error.
    /// Stops at the first failure, leaving that row and the ones after it.
    pub fn delete_files(&self, ids: &[i64]) -> Result<(), String> {
        for &id in ids {
            let d = self.store.get_download(id).map_err(|e| e.to_string())?;
            if let Some(p) = d.target_path.clone().filter(|p| p.exists())
                && let Err(e) = self.recycler.recycle(std::slice::from_ref(&p))
            {
                let name = p.file_name().map_or_else(|| display_name(&d), |n| n.to_string_lossy().into_owned());
                return Err(if in_use(&e) { format!("{name} is in use") } else { format!("{name}: {e}") });
            }
            self.queue.remove(id);
        }
        Ok(())
    }

    /// Queues the same URL again as a new download, into the current download folder.
    pub fn download_again(&self, id: i64) -> Result<i64, EngineError> {
        let d = self.store.get_download(id)?;
        self.queue.add(haul_engine::NewDownload {
            package_id: None,
            url: d.url,
            headers_json: d.headers_json,
            target_dir: self.download_dir(),
            file_name: None,
            priority: Priority::Normal,
            connections: self.connections(),
            speed_limit: None,
        })
    }

    #[cfg(test)]
    fn with_recycler(self, recycler: Arc<dyn Recycler>) -> AppCore {
        AppCore { recycler, ..self }
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
        haul_engine::postproc::try_rules(&facts, rules, categories, sort, &self.download_dir())
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
                target_dir: c.download_dir(),
                file_name: None,
                priority: Priority::Normal,
                connections: c.connections(),
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
        c.set_settings(crate::settings::SettingsPatch { watch_clipboard: Some(true), ..Default::default() }).unwrap();
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
        c.set_settings(crate::settings::SettingsPatch {
            speed_limit: Some(SpeedLimit { enabled: true, bps: 5_000_000 }),
            ..Default::default()
        })
        .unwrap();
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

    #[tokio::test]
    async fn resume_all_requeues_paused_only() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        queue_one(&c);
        queue_one(&c);
        let ids: Vec<i64> = c.store.list_downloads().unwrap().iter().map(|d| d.id).collect();
        c.pause_all();
        c.store.set_state(ids[1], DownloadState::Failed, Some("boom")).unwrap();
        c.resume_all();
        let states: Vec<DownloadState> = c.store.list_downloads().unwrap().iter().map(|d| d.state).collect();
        assert_eq!(states, vec![DownloadState::Queued, DownloadState::Failed]);
    }

    fn patch() -> crate::settings::SettingsPatch {
        crate::settings::SettingsPatch::default()
    }

    #[tokio::test]
    async fn folder_change_applies_to_next_download() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let c = core_fake(Store::open_in_memory().unwrap(), a.path());
        queue_one(&c);
        let s =
            c.set_settings(crate::settings::SettingsPatch { download_dir: Some(b.path().into()), ..patch() }).unwrap();
        assert_eq!(s.download_dir, b.path());
        c.add_links("https://a.org/2.bin", "paste").unwrap();
        let ids: Vec<i64> = wait_checked(&c).await.links.iter().map(|l| l.id).collect();
        c.collector_start(&ids).unwrap();
        let ds = c.store.list_downloads().unwrap();
        let dir_of = |u: &str| ds.iter().find(|d| d.url.ends_with(u)).unwrap().target_dir.clone();
        assert_eq!(dir_of("f.bin"), a.path());
        assert_eq!(dir_of("2.bin"), b.path());
    }

    #[tokio::test]
    async fn connections_apply_to_next_download() {
        let dir = tempfile::tempdir().unwrap();
        let c = core_fake(Store::open_in_memory().unwrap(), dir.path());
        c.set_settings(crate::settings::SettingsPatch { connections: Some(3), ..patch() }).unwrap();
        c.add_links("https://a.org/2.bin", "paste").unwrap();
        let ids: Vec<i64> = wait_checked(&c).await.links.iter().map(|l| l.id).collect();
        c.collector_start(&ids).unwrap();
        assert_eq!(c.store.list_downloads().unwrap()[0].connections, 3);
    }

    #[tokio::test]
    async fn unwritable_folder_refused_nothing_saved() {
        let dir = tempfile::tempdir().unwrap();
        let c = core_fake(Store::open_in_memory().unwrap(), dir.path());
        let e = c
            .set_settings(crate::settings::SettingsPatch {
                download_dir: Some(dir.path().join("gone")),
                max_parallel: Some(7),
                ..patch()
            })
            .unwrap_err();
        assert!(e.to_string().contains("downloadDir:"), "{e}");
        assert_eq!(c.settings().max_parallel, 3);
        assert_eq!(c.settings().download_dir, dir.path());
    }

    #[tokio::test]
    async fn settings_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let c = core_fake(store.clone(), dir.path());
        c.set_settings(crate::settings::SettingsPatch { max_parallel: Some(6), close_to_tray: Some(false), ..patch() })
            .unwrap();
        c.shutdown().await;
        let s = core_fake(store, dir.path()).settings();
        assert_eq!(s.max_parallel, 6);
        assert!(!s.close_to_tray);
    }

    #[tokio::test]
    async fn clipboard_switch_is_the_setting() {
        let dir = tempfile::tempdir().unwrap();
        let c = core_fake(Store::open_in_memory().unwrap(), dir.path());
        c.set_settings(crate::settings::SettingsPatch { watch_clipboard: Some(true), ..patch() }).unwrap();
        assert!(c.watch_clipboard());
        assert!(c.collector_snapshot().unwrap().watch_clipboard);
    }

    #[tokio::test]
    async fn speed_limit_is_the_setting() {
        let dir = tempfile::tempdir().unwrap();
        let c = core_fake(Store::open_in_memory().unwrap(), dir.path());
        let l = SpeedLimit { enabled: true, bps: 5000 };
        c.set_settings(crate::settings::SettingsPatch { speed_limit: Some(l), ..patch() }).unwrap();
        assert_eq!(c.snapshot().unwrap().speed_limit, l);
    }

    #[tokio::test]
    async fn browser_start_collector_sends_capture_to_collector() {
        let dir = tempfile::tempdir().unwrap();
        let c = core_fake(Store::open_in_memory().unwrap(), dir.path());
        c.set_settings(crate::settings::SettingsPatch {
            browser_start: Some(crate::settings::BrowserStart::Collector),
            ..patch()
        })
        .unwrap();
        c.capture(crate::dto::CaptureRequest {
            url: "https://a.org/f.zip".into(),
            filename: None,
            size: None,
            referrer: None,
            user_agent: None,
            cookies: None,
        })
        .unwrap();
        assert!(c.store.list_downloads().unwrap().is_empty());
        let links = wait_checked(&c).await.links;
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://a.org/f.zip");
    }

    #[tokio::test]
    async fn start_with_missing_folder_keeps_the_setting() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("usb");
        std::fs::create_dir(&gone).unwrap();
        let store = Store::open_in_memory().unwrap();
        let c = core_fake(store.clone(), dir.path());
        c.set_settings(crate::settings::SettingsPatch { download_dir: Some(gone.clone()), ..patch() }).unwrap();
        c.shutdown().await;
        std::fs::remove_dir(&gone).unwrap();
        let c = core_fake(store, dir.path());
        assert_eq!(c.settings().download_dir, gone);
        queue_one(&c);
        assert_eq!(c.store.list_downloads().unwrap()[0].target_dir, gone);
    }

    /// Records what it was asked to recycle, or fails every call with `fail`.
    #[derive(Default)]
    struct FakeBin {
        seen: Mutex<Vec<PathBuf>>,
        fail: Option<String>,
    }

    impl haul_engine::postproc::sort::Recycler for FakeBin {
        fn recycle(&self, paths: &[PathBuf]) -> Result<(), String> {
            if let Some(e) = &self.fail {
                return Err(e.clone());
            }
            self.seen.lock().unwrap().extend_from_slice(paths);
            Ok(())
        }
    }

    /// One completed download saved at `<dir>/<name>`, written to disk when `on_disk`.
    fn finished_one(c: &AppCore, dir: &std::path::Path, name: &str, on_disk: bool) -> (i64, PathBuf) {
        queue_one(c);
        let id = c.store.list_downloads().unwrap().last().unwrap().id;
        let path = dir.join(name);
        if on_disk {
            std::fs::write(&path, b"x").unwrap();
        }
        c.store.set_completed(id, &path).unwrap();
        (id, path)
    }

    #[tokio::test]
    async fn snapshot_carries_finished_at() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        queue_one(&c);
        assert_eq!(c.snapshot().unwrap().downloads[0].finished_at, None);
        let (id, _) = finished_one(&c, dir.path(), "a.bin", true);
        let d = c.snapshot().unwrap().downloads.into_iter().find(|d| d.id == id).unwrap();
        assert!(d.finished_at.is_some());
    }

    #[tokio::test]
    async fn files_exist_lists_ids_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        let (a, _) = finished_one(&c, dir.path(), "a.bin", true);
        let (b, _) = finished_one(&c, dir.path(), "b.bin", false);
        assert_eq!(c.files_exist(&[a, b, 999]), vec![a]);
    }

    #[tokio::test]
    async fn delete_files_recycles_and_removes() {
        let dir = tempfile::tempdir().unwrap();
        let bin = Arc::new(FakeBin::default());
        let c = core(Store::open_in_memory().unwrap(), dir.path()).with_recycler(bin.clone());
        let (id, path) = finished_one(&c, dir.path(), "a.bin", true);
        c.delete_files(&[id]).unwrap();
        assert_eq!(*bin.seen.lock().unwrap(), vec![path]);
        assert!(c.store.list_downloads().unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_file_in_use_keeps_row() {
        let dir = tempfile::tempdir().unwrap();
        let bin = Arc::new(FakeBin {
            fail: Some(
                "The process cannot access the file because it is being used by another process. (os error 32)".into(),
            ),
            ..FakeBin::default()
        });
        let c = core(Store::open_in_memory().unwrap(), dir.path()).with_recycler(bin);
        let (a, _) = finished_one(&c, dir.path(), "a.bin", true);
        let (b, _) = finished_one(&c, dir.path(), "b.bin", true);
        let err = c.delete_files(&[a, b]).unwrap_err();
        assert_eq!(err, "a.bin is in use");
        let left: Vec<i64> = c.store.list_downloads().unwrap().iter().map(|d| d.id).collect();
        assert_eq!(left, vec![a, b]);
    }

    #[tokio::test]
    async fn delete_files_missing_file_just_removes_row() {
        let dir = tempfile::tempdir().unwrap();
        let bin = Arc::new(FakeBin::default());
        let c = core(Store::open_in_memory().unwrap(), dir.path()).with_recycler(bin.clone());
        let (id, _) = finished_one(&c, dir.path(), "gone.bin", false);
        c.delete_files(&[id]).unwrap();
        assert!(bin.seen.lock().unwrap().is_empty());
        assert!(c.store.list_downloads().unwrap().is_empty());
    }

    #[tokio::test]
    async fn download_again_uses_same_url() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        let (id, _) = finished_one(&c, dir.path(), "a.bin", true);
        let other = dir.path().join("new");
        std::fs::create_dir(&other).unwrap();
        c.set_settings(crate::settings::SettingsPatch { download_dir: Some(other.clone()), ..patch() }).unwrap();
        let again = c.download_again(id).unwrap();
        assert_ne!(again, id);
        let d = c.store.get_download(again).unwrap();
        assert_eq!(d.url, "https://a.org/f.bin");
        assert_eq!(d.target_dir, other);
        assert_eq!(d.state, DownloadState::Queued);
        assert_eq!(c.store.get_download(id).unwrap().state, DownloadState::Completed);
    }
}
