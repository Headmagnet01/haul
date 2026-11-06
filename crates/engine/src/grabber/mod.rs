//! The Collector: intake, online checks, grouping into packages, duplicates and start.

pub mod dupes;
pub mod grouping;

use crate::error::EngineError;
use crate::events::EventBus;
use crate::naming::filename_from;
use crate::probe::probe;
use crate::queue::Queue;
use crate::store::{Batch, CollectorLink, NewDownload, Store};
use crate::types::{DownloadState, Event, LinkStatus, PackageId, Priority, ProbeInfo};
use dupes::{DupKey, find_duplicates};
use futures::future::BoxFuture;
use grouping::{common_name_prefix, group_names};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

pub use dupes::DupKind;
pub use grouping::ArchiveKind;

/// Probes at most this many links at once across the whole Collector.
pub const PROBE_SLOTS: usize = 8;
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
/// Package keys of archive sets start with this; other keys are `"<batch id>:rest"`.
const ARCHIVE_KEY: &str = "a:";

/// Checks one link. Injected so tests run without a network.
pub type ProbeFn = Arc<dyn Fn(String) -> BoxFuture<'static, Result<ProbeInfo, EngineError>> + Send + Sync>;

/// The real prober: one `probe` with no extra headers, given up after 20 seconds.
pub fn http_prober(client: reqwest::Client) -> ProbeFn {
    Arc::new(move |url: String| {
        let client = client.clone();
        Box::pin(async move {
            let headers = reqwest::header::HeaderMap::new();
            tokio::time::timeout(PROBE_TIMEOUT, probe(&client, &url, &headers))
                .await
                .unwrap_or_else(|_| Err(EngineError::Network("timed out".into())))
        })
    })
}

/// A probe result as the Collector stores it: status, what was learned, and a short reason.
pub fn outcome(r: Result<ProbeInfo, EngineError>) -> (LinkStatus, Option<ProbeInfo>, Option<String>) {
    match r {
        Ok(p) => (LinkStatus::Online, Some(p), None),
        Err(EngineError::Offline) => (LinkStatus::Offline, None, Some("404".into())),
        Err(EngineError::LinkExpired(c)) => (LinkStatus::Expired, None, Some(c.to_string())),
        Err(EngineError::Http(c)) => (LinkStatus::Error, None, Some(format!("HTTP {c}"))),
        Err(e) => (LinkStatus::Error, None, Some(e.to_string())),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectedLink {
    pub id: i64,
    pub batch_id: i64,
    pub url: String,
    pub name: String,
    pub size: Option<u64>,
    pub content_type: Option<String>,
    pub status: LinkStatus,
    pub message: Option<String>,
    pub source: String,
    pub duplicate: Option<DupKind>,
    /// `None` for a lone link.
    pub package_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorPackage {
    /// `"<batch id>:<group key>"`.
    pub key: String,
    pub name: String,
    pub archive: Option<ArchiveKind>,
    pub source: String,
    pub created_at: String,
}

/// The Collector in display order: each package is followed by its links.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct CollectorState {
    pub packages: Vec<CollectorPackage>,
    pub links: Vec<CollectedLink>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakeResult {
    pub added: usize,
    /// Links already waiting in the Collector.
    pub skipped: usize,
}

#[derive(Clone, Debug)]
pub struct StartDefaults {
    pub target_dir: PathBuf,
    pub connections: u32,
}

#[derive(Clone)]
pub struct Grabber {
    store: Store,
    events: EventBus,
    probe: ProbeFn,
    permits: Arc<Semaphore>,
    /// Intake, start, remove and recheck read the Collector and then change it; they take turns,
    /// so two quick Enters or two pastes can't queue or add the same link twice.
    turn: Arc<std::sync::Mutex<()>>,
    /// Lets intake run on threads outside the runtime, such as the clipboard watcher.
    rt: tokio::runtime::Handle,
}

fn link_name(l: &CollectorLink) -> String {
    if let Some(n) = l.probe.as_ref().map(|p| p.file_name.as_str()).filter(|n| !n.is_empty()) {
        return n.to_string();
    }
    match url::Url::parse(&l.url) {
        Ok(u) => filename_from(None, &u),
        Err(_) => l.url.clone(),
    }
}

/// A download as duplicate detection sees it, and whether it has finished.
fn download_key(d: &crate::store::Download) -> (bool, DupKey<'_>) {
    let name = d.file_name.as_deref().or(d.probe.as_ref().map(|p| p.file_name.as_str())).unwrap_or("");
    let key = DupKey {
        urls: std::iter::once(d.url.as_str()).chain(d.probe.as_ref().map(|p| p.final_url.as_str())).collect(),
        name,
        size: d.probe.as_ref().and_then(|p| p.size),
    };
    (d.state == DownloadState::Completed, key)
}

fn pasted_label() -> String {
    format!("Pasted list · {}", chrono::Local::now().format("%-d %b %H:%M"))
}

impl Grabber {
    /// Must be called inside a Tokio runtime. Links left `checking` by the last run are checked again.
    pub fn new(store: Store, events: EventBus, probe: ProbeFn) -> Grabber {
        let g = Grabber {
            store,
            events,
            probe,
            permits: Arc::new(Semaphore::new(PROBE_SLOTS)),
            turn: Arc::new(std::sync::Mutex::new(())),
            rt: tokio::runtime::Handle::current(),
        };
        match g.store.list_links() {
            Ok(links) => {
                links.into_iter().filter(|l| l.status == LinkStatus::Checking).for_each(|l| g.spawn_probe(l.id, l.url))
            }
            Err(e) => tracing::warn!(error = %e, "could not reload collector links"),
        }
        g
    }

    fn take_turn(&self) -> std::sync::MutexGuard<'_, ()> {
        self.turn.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn changed(&self) {
        self.events.send(Event::CollectorChanged);
    }

    fn spawn_probe(&self, id: i64, url: String) {
        let (store, events, probe, permits) =
            (self.store.clone(), self.events.clone(), self.probe.clone(), self.permits.clone());
        self.rt.spawn(async move {
            let Ok(_slot) = permits.acquire_owned().await else { return };
            let (status, info, message) = outcome(probe(url).await);
            if let Err(e) = store.set_link_probe(id, status, info.as_ref(), message.as_deref()) {
                tracing::warn!(id, error = %e, "could not save link check");
            }
            events.send(Event::CollectorChanged);
        });
    }

    /// Adds links as one batch and starts checking them. Links already waiting are skipped.
    pub fn intake(&self, links: Vec<String>, source: &str) -> Result<IntakeResult, EngineError> {
        let _turn = self.take_turn();
        let mut seen: HashSet<String> = self.store.list_links()?.into_iter().map(|l| l.url).collect();
        let total = links.len();
        let fresh: Vec<String> = links.into_iter().filter(|u| seen.insert(u.clone())).collect();
        let skipped = total - fresh.len();
        if fresh.is_empty() {
            return Ok(IntakeResult { added: 0, skipped });
        }
        let batch = self.store.create_batch(source, &pasted_label())?;
        let ids = self.store.insert_links(batch, &fresh)?;
        for (id, url) in ids.into_iter().zip(fresh.iter()) {
            self.spawn_probe(id, url.clone());
        }
        self.changed();
        Ok(IntakeResult { added: fresh.len(), skipped })
    }

    pub fn state(&self) -> Result<CollectorState, EngineError> {
        let batches = self.store.list_batches()?;
        let links = self.store.list_links()?;
        let downloads = self.store.list_downloads()?;
        let names: Vec<String> = links.iter().map(link_name).collect();

        let link_keys: Vec<DupKey> = links
            .iter()
            .zip(&names)
            .map(|(l, n)| DupKey {
                urls: std::iter::once(l.url.as_str()).chain(l.probe.as_ref().map(|p| p.final_url.as_str())).collect(),
                name: n,
                size: l.probe.as_ref().and_then(|p| p.size),
            })
            .collect();
        let (finished, queued): (Vec<_>, Vec<_>) = downloads.iter().map(download_key).partition(|(done, _)| *done);
        let finished: Vec<DupKey> = finished.into_iter().map(|(_, k)| k).collect();
        let queued: Vec<DupKey> = queued.into_iter().map(|(_, k)| k).collect();
        let dups = find_duplicates(&link_keys, &queued, &finished);

        // Archive sets group across every waiting link: clipboard users copy parts one at a time.
        let all_names: Vec<&str> = names.iter().map(String::as_str).collect();
        let sets: Vec<grouping::Group> = group_names(&all_names).into_iter().filter(|g| g.archive.is_some()).collect();
        let mut in_set = vec![false; links.len()];
        sets.iter().flat_map(|g| &g.members).for_each(|&i| in_set[i] = true);
        // Each set is shown where its earliest link arrived.
        let anchor = |g: &grouping::Group| g.members.iter().map(|&i| links[i].batch_id).min().unwrap_or(0);

        let mut by_batch: HashMap<i64, Vec<usize>> = HashMap::new();
        for (i, l) in links.iter().enumerate() {
            if !in_set[i] {
                by_batch.entry(l.batch_id).or_default().push(i);
            }
        }
        let mut out = CollectorState::default();
        for b in &batches {
            for g in sets.iter().filter(|g| anchor(g) == b.id) {
                let key = format!("{ARCHIVE_KEY}{}", g.key);
                out.packages.push(CollectorPackage {
                    key: key.clone(),
                    name: g.base.clone().unwrap_or_default(),
                    archive: g.archive,
                    source: b.source.clone(),
                    created_at: b.created_at.clone(),
                });
                for &i in &g.members {
                    let lb = batches.iter().find(|x| x.id == links[i].batch_id).unwrap_or(b);
                    out.links.push(collected(&links[i], &names[i], lb, dups[i], Some(key.clone())));
                }
            }
            let Some(rest) = by_batch.get(&b.id) else { continue };
            let package_key = (rest.len() >= 2).then(|| {
                let key = format!("{}:rest", b.id);
                let ns: Vec<&str> = rest.iter().map(|&i| names[i].as_str()).collect();
                out.packages.push(CollectorPackage {
                    key: key.clone(),
                    name: common_name_prefix(&ns).unwrap_or_else(|| b.label.clone()),
                    archive: None,
                    source: b.source.clone(),
                    created_at: b.created_at.clone(),
                });
                key
            });
            for &i in rest {
                out.links.push(collected(&links[i], &names[i], b, dups[i], package_key.clone()));
            }
        }
        Ok(out)
    }

    /// Queues the given links (offline and expired ones stay) and removes them from the Collector.
    pub fn start(&self, ids: &[i64], queue: &Queue, d: &StartDefaults) -> Result<usize, EngineError> {
        let _turn = self.take_turn();
        let wanted: HashSet<i64> = ids.iter().copied().collect();
        let probes: HashMap<i64, ProbeInfo> =
            self.store.list_links()?.into_iter().filter_map(|l| Some((l.id, l.probe?))).collect();
        let state = self.state()?;
        let packages: HashMap<&str, &CollectorPackage> = state.packages.iter().map(|p| (p.key.as_str(), p)).collect();
        let mut made: HashMap<String, PackageId> = HashMap::new();
        let mut started = Vec::new();
        for l in &state.links {
            if !wanted.contains(&l.id) || matches!(l.status, LinkStatus::Offline | LinkStatus::Expired) {
                continue;
            }
            let package_id = match l.package_key.as_deref() {
                None => None,
                Some(key) => Some(match made.get(key) {
                    Some(&id) => id,
                    None => {
                        let id = self.package_for(key, l.batch_id, packages.get(key).copied())?;
                        made.insert(key.to_string(), id);
                        id
                    }
                }),
            };
            let id = queue.add(NewDownload {
                package_id,
                url: l.url.clone(),
                headers_json: None,
                target_dir: d.target_dir.clone(),
                file_name: None,
                priority: Priority::Normal,
                connections: d.connections,
                speed_limit: None,
            })?;
            if let Some(p) = probes.get(&l.id).filter(|_| l.status == LinkStatus::Online) {
                self.store.set_probe(id, p)?;
            }
            started.push(l.id);
        }
        if !started.is_empty() {
            self.store.delete_links(&started)?;
            self.changed();
        }
        Ok(started.len())
    }

    /// The Queue package a collector package became, created on its first start.
    fn package_for(&self, key: &str, batch_id: i64, pkg: Option<&CollectorPackage>) -> Result<PackageId, EngineError> {
        let (name, source) = pkg.map_or(("Pasted list", "paste"), |p| (p.name.as_str(), p.source.as_str()));
        if let Some(set) = key.strip_prefix(ARCHIVE_KEY) {
            if let Some(id) = self.store.archive_package(set)? {
                return Ok(id);
            }
            let id = self.store.create_package(name, source)?;
            self.store.set_archive_package(set, id)?;
            return Ok(id);
        }
        let group_key = key.split_once(':').map_or(key, |(_, g)| g);
        if let Some(id) = self.store.group_package(batch_id, group_key)? {
            return Ok(id);
        }
        let id = self.store.create_package(name, source)?;
        self.store.set_group_package(batch_id, group_key, id)?;
        Ok(id)
    }

    pub fn remove(&self, ids: &[i64]) -> Result<(), EngineError> {
        let _turn = self.take_turn();
        self.store.delete_links(ids)?;
        self.changed();
        Ok(())
    }

    /// Checks the given links again.
    pub fn recheck(&self, ids: &[i64]) -> Result<(), EngineError> {
        let _turn = self.take_turn();
        let wanted: HashSet<i64> = ids.iter().copied().collect();
        for l in self.store.list_links()?.into_iter().filter(|l| wanted.contains(&l.id)) {
            self.store.set_link_probe(l.id, LinkStatus::Checking, None, None)?;
            self.spawn_probe(l.id, l.url);
        }
        self.changed();
        Ok(())
    }
}

fn collected(
    l: &CollectorLink,
    name: &str,
    b: &Batch,
    duplicate: Option<DupKind>,
    key: Option<String>,
) -> CollectedLink {
    CollectedLink {
        id: l.id,
        batch_id: l.batch_id,
        url: l.url.clone(),
        name: name.to_string(),
        size: l.probe.as_ref().and_then(|p| p.size),
        content_type: l.probe.as_ref().and_then(|p| p.content_type.clone()),
        status: l.status,
        message: l.message.clone(),
        source: b.source.clone(),
        duplicate,
        package_key: key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::EngineError;
    use crate::types::{LinkStatus, ProbeInfo};

    #[test]
    fn outcome_mapping() {
        let p = ProbeInfo {
            final_url: "https://a/x".into(),
            size: None,
            resumable: false,
            file_name: "x".into(),
            etag: None,
            last_modified: None,
            content_type: None,
            digest: None,
        };
        assert_eq!(outcome(Ok(p.clone())), (LinkStatus::Online, Some(p), None));
        assert_eq!(outcome(Err(EngineError::Offline)), (LinkStatus::Offline, None, Some("404".into())));
        assert_eq!(outcome(Err(EngineError::LinkExpired(410))), (LinkStatus::Expired, None, Some("410".into())));
        assert_eq!(outcome(Err(EngineError::Http(503))), (LinkStatus::Error, None, Some("HTTP 503".into())));
        assert_eq!(
            outcome(Err(EngineError::Network("timed out".into()))),
            (LinkStatus::Error, None, Some("network: timed out".into()))
        );
    }
}
