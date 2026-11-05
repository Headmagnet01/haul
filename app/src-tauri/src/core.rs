//! App commands as plain methods on `AppCore`, so they test without Tauri.

use crate::dto::{AddResult, DownloadView, PackageView, Snapshot, SpeedLimit};
use haul_engine::{
    Download, DownloadState, EngineError, EventBus, HostLimiter, NewDownload, Priority, Queue, RateLimiter, Schedule,
    Store, TransferConfig, TransferCtx, extract_links, naming,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const KEY_SPEED_LIMIT: &str = "speed_limit";
pub const KEY_DOWNLOAD_DIR: &str = "download_dir";
pub const KEY_MAX_PARALLEL: &str = "max_parallel";
const DEFAULT_MAX_PARALLEL: usize = 3;
const DEFAULT_CONNECTIONS: u32 = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FolderTarget {
    RevealFile(PathBuf),
    OpenDir(PathBuf),
}

pub struct AppCore {
    store: Store,
    ctx: TransferCtx,
    queue: Queue,
    download_dir: PathBuf,
}

fn priority_str(p: Priority) -> &'static str {
    match p {
        Priority::High => "high",
        Priority::Normal => "normal",
        Priority::Low => "low",
    }
}

fn parse_priority(s: &str) -> Result<Priority, EngineError> {
    match s {
        "high" => Ok(Priority::High),
        "normal" => Ok(Priority::Normal),
        "low" => Ok(Priority::Low),
        _ => Err(EngineError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "unknown priority"))),
    }
}

fn display_name(d: &Download) -> String {
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
    pub fn start(db_path: &Path, default_download_dir: PathBuf) -> Result<AppCore, EngineError> {
        Ok(Self::start_with(Store::open(db_path)?, default_download_dir, None))
    }

    /// `schedule` lets tests keep the queue from ever starting a download.
    pub fn start_with(store: Store, default_download_dir: PathBuf, schedule: Option<Schedule>) -> AppCore {
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
        AppCore { store, ctx, queue, download_dir }
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

    fn view(&self, d: Download) -> Result<DownloadView, EngineError> {
        let segs = self.store.load_segments(d.id)?;
        let size = d.probe.as_ref().and_then(|p| p.size);
        let sum: u64 = segs.iter().map(|r| r.pos.saturating_sub(r.start)).sum();
        let written = if d.state == DownloadState::Completed { size.unwrap_or(sum) } else { sum };
        Ok(DownloadView {
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
        })
    }

    pub fn snapshot(&self) -> Result<Snapshot, EngineError> {
        let downloads =
            self.store.list_downloads()?.into_iter().map(|d| self.view(d)).collect::<Result<Vec<_>, _>>()?;
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

    /// Queues every link in `text`. Two or more links share a new "Pasted list" package.
    pub fn add_links(&self, text: &str) -> Result<AddResult, EngineError> {
        let links = extract_links(text);
        if links.is_empty() {
            return Ok(AddResult { added: 0, package_id: None });
        }
        let package_id = if links.len() >= 2 {
            let name = format!("Pasted list · {}", chrono::Local::now().format("%-d %b %H:%M"));
            Some(self.store.create_package(&name, "paste")?)
        } else {
            None
        };
        for url in &links {
            self.queue.add(NewDownload {
                package_id,
                url: url.clone(),
                headers_json: None,
                target_dir: self.download_dir.clone(),
                file_name: None,
                priority: Priority::Normal,
                connections: DEFAULT_CONNECTIONS,
                speed_limit: None,
            })?;
        }
        Ok(AddResult { added: links.len(), package_id })
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

    #[tokio::test]
    async fn add_one_link_no_package() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        let r = c.add_links("x https://a.org/f.bin y").unwrap();
        assert_eq!(r.added, 1);
        assert_eq!(r.package_id, None);
        let s = c.snapshot().unwrap();
        assert_eq!(s.downloads.len(), 1);
        let d = &s.downloads[0];
        assert_eq!(d.name, "f.bin");
        assert_eq!(d.state, "queued");
        assert_eq!(d.priority, "normal");
        assert_eq!(d.host, "a.org");
        assert!(d.segments.is_empty());
        assert_eq!(d.written, 0);
        assert_eq!(d.connections, 8);
        assert_eq!(d.target_dir, dir.path().to_string_lossy());
        assert!(s.packages.is_empty());
    }

    #[tokio::test]
    async fn add_many_links_one_package() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        let r = c.add_links("https://a.org/1.bin\nhttps://a.org/2.bin\nhttps://a.org/3.bin").unwrap();
        assert_eq!(r.added, 3);
        let pid = r.package_id.expect("package");
        let s = c.snapshot().unwrap();
        assert_eq!(s.packages.len(), 1);
        assert_eq!(s.packages[0].id, pid);
        assert!(s.packages[0].name.starts_with("Pasted list · "), "{}", s.packages[0].name);
        assert_eq!(s.downloads.len(), 3);
        assert!(s.downloads.iter().all(|d| d.package_id == Some(pid)));
    }

    #[tokio::test]
    async fn add_no_links() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        let r = c.add_links("nothing here").unwrap();
        assert_eq!(r.added, 0);
        assert_eq!(r.package_id, None);
        assert!(c.snapshot().unwrap().packages.is_empty());
        assert!(c.store.list_packages().unwrap().is_empty());
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
        c.add_links("https://a.org/f.bin").unwrap();
        let id = c.snapshot().unwrap().downloads[0].id;
        assert!(c.set_priority(id, "urgent").is_err());
        c.set_priority(id, "high").unwrap();
        assert_eq!(c.snapshot().unwrap().downloads[0].priority, "high");
    }

    #[tokio::test]
    async fn folder_target_for_unfinished_is_dir() {
        let dir = tempfile::tempdir().unwrap();
        let c = core(Store::open_in_memory().unwrap(), dir.path());
        c.add_links("https://a.org/f.bin").unwrap();
        let id = c.snapshot().unwrap().downloads[0].id;
        assert_eq!(c.folder_target(id).unwrap(), FolderTarget::OpenDir(dir.path().to_path_buf()));
    }
}
