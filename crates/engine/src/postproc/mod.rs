//! Post-processing: what happens to a package after every file in it has finished.
//!
//! `PostProc` watches the event bus. When the last download of a package (or a lone download)
//! completes, it makes a run, and one worker executes runs one at a time: verify, extract, sort,
//! scan, notify. Every change is saved, so a restart carries on from the first unfinished step.

pub mod checksum;
pub mod extract;
mod run;
pub mod scan;
pub mod sort;

pub use run::{Run, RunState, Step, StepKind, StepState};

use crate::error::EngineError;
use crate::events::EventBus;
use crate::grabber::grouping::archive_part;
use crate::naming::reserve_unique_dir;
use crate::queue::Queue;
use crate::rules::{
    Category, Destination, Facts, Rule, RuleAction, RuleMatch, decide, default_categories, validate_rule,
};
use crate::store::{Download, RunRow, Store};
use crate::types::{DownloadId, DownloadState, Event, PackageId};
use checksum::{Algo, hash_file, is_checksum_file, parse_checksum_file, parse_pasted, parse_tagged, tagged};
use extract::{ExtractOutcome, Extractor, first_volumes, is_archive_name};
use run::RunData;
use scan::{ScanOutcome, Scanner};
use serde::{Deserialize, Serialize};
use sort::{Recycler, move_item};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

pub const KEY_SETTINGS: &str = "postproc";
pub const KEY_CATEGORIES: &str = "categories";

/// The four switches in the Rules view (settings key `postproc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PostProcSettings {
    pub auto: bool,
    pub sort_categories: bool,
    pub recycle_archives: bool,
    pub virus_scan: bool,
}

impl Default for PostProcSettings {
    fn default() -> Self {
        PostProcSettings { auto: true, sort_categories: true, recycle_archives: false, virus_scan: false }
    }
}

/// A Windows notification to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
    pub folder: Option<PathBuf>,
}

/// The parts that touch the operating system, injected so tests can fake them.
#[derive(Clone)]
pub struct PostProcDeps {
    pub extractor: Arc<dyn Extractor>,
    pub recycler: Arc<dyn Recycler>,
    pub scanner: Arc<dyn Scanner>,
    pub notify: Arc<dyn Fn(Notice) + Send + Sync>,
}

#[derive(Clone)]
pub struct PostProc {
    inner: Arc<Inner>,
}

struct Inner {
    store: Store,
    events: EventBus,
    queue: Queue,
    deps: PostProcDeps,
    download_dir: std::sync::RwLock<PathBuf>,
    wake: Notify,
    /// A password typed in the drawer, tried once on the next run of that extract step.
    given: Mutex<HashMap<i64, (String, bool)>>,
    /// Changes to a run's state from outside the worker take turns with each other.
    turn: Mutex<()>,
}

enum Outcome {
    Done(String),
    Skipped(String),
    Failed(String),
    NeedsPassword(String),
    /// Stop here until a re-downloaded file finishes.
    Waiting(String),
}

/// What one run works on.
struct Target {
    name: String,
    dir: PathBuf,
    members: Vec<Download>,
}

fn invalid(msg: &str) -> EngineError {
    EngineError::Invalid(msg.to_string())
}

fn file_name_of(d: &Download) -> String {
    d.target_path
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .or_else(|| d.file_name.clone())
        .unwrap_or_default()
}

fn host_of(d: &Download) -> String {
    let url = d.probe.as_ref().map_or(d.url.as_str(), |p| p.final_url.as_str());
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default()
}

/// A name without its last extension; an archive part's set name.
fn base_name(file_name: &str) -> String {
    if let Some(p) = archive_part(file_name) {
        return p.base;
    }
    match file_name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => file_name.to_string(),
    }
}

fn tmp_of(out: &Path) -> PathBuf {
    let mut name = out.file_name().unwrap_or_default().to_os_string();
    name.push(".haul-tmp");
    out.with_file_name(name)
}

/// Every file under `dir`, recursively, sorted.
fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn remove_if_empty(dir: &Path) {
    if std::fs::read_dir(dir).is_ok_and(|mut r| r.next().is_none()) {
        let _ = std::fs::remove_dir(dir);
    }
}

/// Validates the rules first, then decides. For the Rules view's "Try a file name" box.
pub fn try_rules(
    facts: &Facts,
    rules: &[Rule],
    cats: &[Category],
    sort: bool,
    base: &Path,
) -> Result<Destination, String> {
    for r in rules {
        validate_rule(r)?;
    }
    Ok(decide(facts, rules, cats, sort, base))
}

fn to_run(r: &RunRow) -> Run {
    let data: RunData = serde_json::from_str(&r.steps_json).unwrap_or_else(|_| RunData::fresh());
    let mut steps = RunData::fresh().steps;
    for s in &mut steps {
        if let Some(saved) = data.step(s.kind) {
            *s = saved.clone();
        }
    }
    Run {
        id: r.id,
        package_id: r.package_id,
        download_id: r.download_id,
        state: RunState::parse(&r.state).unwrap_or(RunState::Failed),
        steps,
        updated_at: r.updated_at.clone(),
    }
}

impl PostProc {
    /// Subscribes to `events` before returning, recovers runs a crash left running, and starts the worker.
    pub fn start(store: Store, events: EventBus, queue: Queue, deps: PostProcDeps, download_dir: PathBuf) -> PostProc {
        let mut rx = events.subscribe();
        let inner = Arc::new(Inner {
            store,
            events,
            queue,
            deps,
            download_dir: std::sync::RwLock::new(download_dir),
            wake: Notify::new(),
            given: Mutex::new(HashMap::new()),
            turn: Mutex::new(()),
        });
        inner.recover();
        let listener = inner.clone();
        tokio::spawn(async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                match rx.recv().await {
                    Ok(Event::StateChanged { id, state, .. }) => match state {
                        DownloadState::Completed => listener.on_completed(id),
                        DownloadState::Failed | DownloadState::Offline | DownloadState::LinkExpired => {
                            listener.on_failed(id)
                        }
                        _ => {}
                    },
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        });
        let worker = inner.clone();
        tokio::spawn(async move {
            loop {
                match worker.claim_next() {
                    Some(id) => {
                        let w = worker.clone();
                        if let Err(e) = tokio::task::spawn_blocking(move || w.execute(id)).await {
                            tracing::warn!(run = id, error = %e, "post-processing task panicked");
                        }
                    }
                    None => {
                        tokio::select! {
                            _ = worker.wake.notified() => {}
                            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                        }
                    }
                }
            }
        });
        PostProc { inner }
    }

    pub fn runs(&self) -> Result<Vec<Run>, EngineError> {
        Ok(self.inner.store.list_runs()?.iter().map(to_run).collect())
    }

    /// Makes or gets the run of a finished package (or lone download) and queues it.
    pub fn process(&self, package_id: Option<PackageId>, download_id: Option<DownloadId>) -> Result<i64, EngineError> {
        let i = &self.inner;
        let members = i.members(package_id, download_id)?;
        if members.is_empty() || members.iter().any(|d| d.state != DownloadState::Completed) {
            return Err(invalid("Not finished yet"));
        }
        let _turn = i.turn.lock().unwrap();
        let fresh = serde_json::to_string(&RunData::fresh()).unwrap_or_default();
        let (id, new) = i.store.ensure_run(package_id, download_id, RunState::Queued.as_str(), &fresh)?;
        if !new {
            i.requeue(id)?;
        }
        i.changed(id);
        i.wake.notify_one();
        Ok(id)
    }

    /// Runs a failed, held or password-blocked run again from its first unfinished step.
    pub fn retry(&self, run_id: i64) -> Result<(), EngineError> {
        let i = &self.inner;
        let _turn = i.turn.lock().unwrap();
        let row = i.store.get_run(run_id)?;
        match RunState::parse(&row.state) {
            Some(RunState::Failed | RunState::Held | RunState::NeedsPassword) => {}
            _ => return Err(invalid("Nothing to retry")),
        }
        i.requeue(run_id)?;
        i.changed(run_id);
        i.wake.notify_one();
        Ok(())
    }

    /// Tries `password` (then the saved ones) on the archive that needs one.
    pub fn give_password(&self, run_id: i64, password: String, remember: bool) -> Result<(), EngineError> {
        self.inner.given.lock().unwrap().insert(run_id, (password, remember));
        self.retry(run_id)
    }

    /// Stores a pasted checksum for a download (`None` or blank clears it).
    pub fn set_expected_hash(&self, download_id: DownloadId, hash: Option<&str>) -> Result<(), EngineError> {
        let v = match hash.map(str::trim).filter(|h| !h.is_empty()) {
            None => None,
            Some(h) => {
                let (algo, hex) = parse_pasted(h).map_err(EngineError::Invalid)?;
                Some(tagged(algo, &hex))
            }
        };
        self.inner.store.get_download(download_id)?;
        self.inner.store.set_expected_hash(download_id, v.as_deref())
    }

    pub fn settings(&self) -> PostProcSettings {
        self.inner.settings()
    }

    pub fn set_settings(&self, s: PostProcSettings) -> Result<(), EngineError> {
        let v = serde_json::to_value(s).map_err(|e| invalid(&e.to_string()))?;
        self.inner.store.set_setting(KEY_SETTINGS, &v)
    }

    pub fn categories(&self) -> Result<Vec<Category>, EngineError> {
        Ok(self.inner.categories())
    }

    pub fn set_categories(&self, cats: Vec<Category>) -> Result<(), EngineError> {
        if cats.is_empty() {
            return Err(invalid("Keep at least one category"));
        }
        if cats.iter().any(|c| c.name.trim().is_empty()) {
            return Err(invalid("Every category needs a name"));
        }
        let cats: Vec<Category> = cats
            .into_iter()
            .map(|c| Category {
                name: c.name.trim().to_string(),
                exts: c
                    .exts
                    .iter()
                    .map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase())
                    .filter(|e| !e.is_empty())
                    .collect(),
                folder: c.folder.trim().to_string(),
            })
            .collect();
        let v = serde_json::to_value(cats).map_err(|e| invalid(&e.to_string()))?;
        self.inner.store.set_setting(KEY_CATEGORIES, &v)
    }

    pub fn rules(&self) -> Result<Vec<Rule>, EngineError> {
        self.inner.rules()
    }

    /// Replaces every rule. The first invalid rule's message is returned and nothing is saved.
    pub fn set_rules(&self, rules: Vec<Rule>) -> Result<(), EngineError> {
        for r in &rules {
            validate_rule(r).map_err(EngineError::Invalid)?;
        }
        let rows: Vec<_> = rules
            .iter()
            .map(|r| {
                (
                    r.enabled,
                    r.name.trim().to_string(),
                    serde_json::to_string(&r.matcher).unwrap_or_else(|_| "{}".into()),
                    serde_json::to_string(&r.action).unwrap_or_else(|_| "{}".into()),
                )
            })
            .collect();
        self.inner.store.set_rules_json(&rows)
    }

    /// Saves a password, encrypted for this Windows user, at the end of the list.
    pub fn add_password(&self, pw: &str) -> Result<i64, EngineError> {
        if pw.is_empty() {
            return Err(invalid("Type a password first"));
        }
        self.inner.store.add_password(&crate::secret::protect(pw.as_bytes())?)
    }

    /// The saved passwords' ids, in the order they are tried. The passwords never leave the engine.
    pub fn passwords(&self) -> Result<Vec<i64>, EngineError> {
        Ok(self.inner.store.list_passwords()?.into_iter().map(|(id, _)| id).collect())
    }

    pub fn remove_password(&self, id: i64) -> Result<(), EngineError> {
        self.inner.store.delete_password(id)
    }

    pub fn order_passwords(&self, ids: &[i64]) -> Result<(), EngineError> {
        self.inner.store.order_passwords(ids)
    }

    pub fn download_dir(&self) -> PathBuf {
        self.inner.download_dir.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The folder relative rule and category folders sit in, from now on.
    pub fn set_download_dir(&self, dir: PathBuf) {
        *self.inner.download_dir.write().unwrap_or_else(|e| e.into_inner()) = dir;
    }
}

impl Inner {
    fn settings(&self) -> PostProcSettings {
        self.store
            .get_setting(KEY_SETTINGS)
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    fn categories(&self) -> Vec<Category> {
        self.store
            .get_setting(KEY_CATEGORIES)
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value::<Vec<Category>>(v).ok())
            .filter(|c| !c.is_empty())
            .unwrap_or_else(default_categories)
    }

    fn rules(&self) -> Result<Vec<Rule>, EngineError> {
        Ok(self
            .store
            .get_rules_json()?
            .into_iter()
            .map(|(id, enabled, name, m, a)| Rule {
                id,
                name,
                enabled,
                matcher: serde_json::from_str::<RuleMatch>(&m).unwrap_or_default(),
                action: serde_json::from_str::<RuleAction>(&a).unwrap_or_default(),
            })
            .collect())
    }

    fn changed(&self, run_id: i64) {
        self.events.send(Event::PostProcChanged { run_id });
    }

    fn load(&self, id: i64) -> Result<(RunRow, RunData), EngineError> {
        let row = self.store.get_run(id)?;
        let data = serde_json::from_str(&row.steps_json).unwrap_or_else(|_| RunData::fresh());
        Ok((row, data))
    }

    fn save(&self, id: i64, state: RunState, data: &RunData) {
        let json = serde_json::to_string(data).unwrap_or_default();
        if let Err(e) = self.store.save_run(id, state.as_str(), &json) {
            tracing::warn!(run = id, error = %e, "could not save post-processing run");
        }
        self.changed(id);
    }

    /// Unfinished steps back to pending, and the run back in line.
    fn requeue(&self, id: i64) -> Result<(), EngineError> {
        let (_, mut data) = self.load(id)?;
        for s in data.steps.iter_mut().filter(|s| !s.state.finished()) {
            s.state = StepState::Pending;
        }
        let json = serde_json::to_string(&data).unwrap_or_default();
        self.store.save_run(id, RunState::Queued.as_str(), &json)
    }

    /// Runs a crash left `running` go back in line; a half-done extraction is cleared away.
    fn recover(&self) {
        let Ok(rows) = self.store.list_runs() else { return };
        for row in rows.into_iter().filter(|r| r.state == RunState::Running.as_str()) {
            let Ok((_, mut data)) = self.load(row.id) else { continue };
            let extract_done = data.step(StepKind::Extract).is_some_and(|s| s.state.finished());
            if !extract_done && let Some(out) = data.extracted_to.take().map(PathBuf::from) {
                let tmp = tmp_of(&out);
                if tmp.exists()
                    && let Err(e) = std::fs::remove_dir_all(&tmp)
                {
                    tracing::warn!(path = %tmp.display(), error = %e, "could not clear an interrupted extraction");
                }
                remove_if_empty(&out);
            }
            for s in data.steps.iter_mut().filter(|s| s.state == StepState::Running) {
                s.state = StepState::Pending;
            }
            let json = serde_json::to_string(&data).unwrap_or_default();
            let _ = self.store.save_run(row.id, RunState::Queued.as_str(), &json);
        }
    }

    fn members(
        &self,
        package_id: Option<PackageId>,
        download_id: Option<DownloadId>,
    ) -> Result<Vec<Download>, EngineError> {
        match (package_id, download_id) {
            (Some(p), _) => Ok(self.store.list_downloads()?.into_iter().filter(|d| d.package_id == Some(p)).collect()),
            (None, Some(d)) => Ok(vec![self.store.get_download(d)?]),
            (None, None) => Ok(Vec::new()),
        }
    }

    fn target(&self, row: &RunRow) -> Result<Target, EngineError> {
        let members = self.members(row.package_id, row.download_id)?;
        let name = match row.package_id {
            Some(p) => self.store.list_packages()?.into_iter().find(|x| x.id == p).map(|x| x.name),
            None => members.first().map(|d| base_name(&file_name_of(d))),
        }
        .unwrap_or_else(|| "download".into());
        let dir = members
            .iter()
            .find_map(|d| d.target_path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf))
            .or_else(|| members.first().map(|d| d.target_dir.clone()))
            .unwrap_or_else(|| self.download_dir.read().unwrap_or_else(|e| e.into_inner()).clone());
        Ok(Target { name: crate::naming::sanitize_filename(&name), dir, members })
    }

    fn on_completed(&self, id: DownloadId) {
        let Ok(d) = self.store.get_download(id) else { return };
        let (pkg, did) = match d.package_id {
            Some(p) => (Some(p), None),
            None => (None, Some(id)),
        };
        let Ok(members) = self.members(pkg, did) else { return };
        if members.is_empty() || members.iter().any(|m| m.state != DownloadState::Completed) {
            return;
        }
        let _turn = self.turn.lock().unwrap();
        let initial = if self.settings().auto { RunState::Queued } else { RunState::Held };
        let fresh = serde_json::to_string(&RunData::fresh()).unwrap_or_default();
        let Ok((run_id, new)) = self.store.ensure_run(pkg, did, initial.as_str(), &fresh) else { return };
        if !new {
            let Ok(row) = self.store.get_run(run_id) else { return };
            if row.state != RunState::Waiting.as_str() || self.requeue(run_id).is_err() {
                return;
            }
        }
        self.changed(run_id);
        self.wake.notify_one();
    }

    /// A file being downloaded again after a mismatch failed: its run fails verify.
    fn on_failed(&self, id: DownloadId) {
        let Ok(rows) = self.store.list_runs() else { return };
        let _turn = self.turn.lock().unwrap();
        for row in rows.into_iter().filter(|r| r.state == RunState::Waiting.as_str()) {
            let Ok(members) = self.members(row.package_id, row.download_id) else { continue };
            let Some(d) = members.iter().find(|m| m.id == id) else { continue };
            let Ok((_, mut data)) = self.load(row.id) else { continue };
            let detail = format!("Checksum mismatch: {}", file_name_of(d));
            let s = data.step_mut(StepKind::Verify);
            s.state = StepState::Failed;
            s.detail = Some(detail.clone());
            self.save(row.id, RunState::Failed, &data);
            if let Ok(t) = self.target(&row) {
                self.attention(&t, &detail);
            }
        }
    }

    /// The oldest queued run, marked running.
    fn claim_next(&self) -> Option<i64> {
        let _turn = self.turn.lock().unwrap();
        let row = self.store.list_runs().ok()?.into_iter().find(|r| r.state == RunState::Queued.as_str())?;
        self.store.save_run(row.id, RunState::Running.as_str(), &row.steps_json).ok()?;
        self.changed(row.id);
        Some(row.id)
    }

    fn attention(&self, t: &Target, detail: &str) {
        (self.deps.notify)(Notice {
            title: format!("{} needs attention", t.name),
            body: detail.to_string(),
            folder: Some(t.dir.clone()),
        });
    }

    fn execute(&self, id: i64) {
        let Ok((row, mut data)) = self.load(id) else { return };
        let mut target = match self.target(&row) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(run = id, error = %e, "post-processing target is gone");
                return;
            }
        };
        for kind in StepKind::ALL {
            if data.step(kind).is_some_and(|s| s.state.finished()) {
                continue;
            }
            {
                let s = data.step_mut(kind);
                s.state = StepState::Running;
                s.detail = None;
            }
            self.save(id, RunState::Running, &data);
            let outcome = match kind {
                StepKind::Verify => self.verify(&target, &mut data),
                StepKind::Extract => self.extract(id, &target, &mut data),
                StepKind::Sort => self.sort(&mut target, &mut data),
                StepKind::Scan => self.scan(&target, &data),
                StepKind::Notify => self.notify(&target, &data),
            };
            let (state, detail, stop) = match outcome {
                Outcome::Done(d) => (StepState::Done, d, None),
                Outcome::Skipped(d) => (StepState::Skipped, d, None),
                Outcome::Failed(d) => (StepState::Failed, d, Some(RunState::Failed)),
                Outcome::NeedsPassword(d) => (StepState::NeedsPassword, d, Some(RunState::NeedsPassword)),
                Outcome::Waiting(d) => (StepState::Pending, d, Some(RunState::Waiting)),
            };
            let s = data.step_mut(kind);
            s.state = state;
            s.detail = Some(detail.clone());
            if let Some(run_state) = stop {
                self.save(id, run_state, &data);
                if run_state != RunState::Waiting {
                    self.attention(&target, &detail);
                }
                return;
            }
            self.save(id, RunState::Running, &data);
        }
        self.save(id, RunState::Done, &data);
    }

    fn verify(&self, t: &Target, data: &mut RunData) -> Outcome {
        // listed name (lowercase) -> (algo, hex, the checksum file's name)
        let mut from_files: HashMap<String, (Algo, String, String)> = HashMap::new();
        for d in t.members.iter().filter(|d| is_checksum_file(&file_name_of(d))) {
            let Some(path) = &d.target_path else { continue };
            let Ok(bytes) = std::fs::read(path) else { continue };
            let text = String::from_utf8_lossy(&bytes);
            for (listed, algo, hex) in parse_checksum_file(&file_name_of(d), &text) {
                let name = listed.rsplit(['/', '\\']).next().unwrap_or(&listed).to_lowercase();
                from_files.entry(name).or_insert((algo, hex, file_name_of(d)));
            }
        }
        let mut first_source: Option<String> = None;
        for d in t.members.iter().filter(|d| !is_checksum_file(&file_name_of(d))) {
            let name = file_name_of(d);
            let expected = from_files
                .get(&name.to_lowercase())
                .cloned()
                .or_else(|| {
                    d.expected_hash.as_deref().and_then(parse_tagged).map(|(a, h)| (a, h, "your checksum".into()))
                })
                .or_else(|| {
                    d.server_digest.as_deref().and_then(parse_tagged).map(|(a, h)| (a, h, "the server".into()))
                });
            let Some((algo, hex, source)) = expected else { continue };
            let label = format!("{} from {}", algo.label(), source);
            if data.verified.contains(&d.id) {
                first_source.get_or_insert(label);
                continue;
            }
            let Some(path) = d.target_path.as_ref().filter(|p| p.is_file()) else {
                return Outcome::Failed(format!("File missing: {name}"));
            };
            let got = match hash_file(path, algo) {
                Ok(h) => h,
                Err(e) => return Outcome::Failed(format!("Couldn't read {name}: {e}")),
            };
            if got == hex {
                data.verified.push(d.id);
                first_source.get_or_insert(label);
                continue;
            }
            if d.verify_retries == 0 {
                let _ = self.store.bump_verify_retries(d.id);
                if let Err(e) = self.deps.recycler.recycle(std::slice::from_ref(path)) {
                    tracing::warn!(path = %path.display(), error = %e, "could not recycle a file that failed its checksum");
                }
                self.queue.restart(d.id);
                return Outcome::Waiting(format!("Downloading {name} again"));
            }
            return Outcome::Failed(format!("Checksum mismatch: {name}"));
        }
        match first_source {
            Some(src) => Outcome::Done(src),
            None => Outcome::Skipped("No checksum".into()),
        }
    }

    fn saved_passwords(&self) -> Vec<String> {
        self.store
            .list_passwords()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(_, blob)| crate::secret::unprotect(&blob).ok())
            .filter_map(|b| String::from_utf8(b).ok())
            .collect()
    }

    fn extract(&self, run_id: i64, t: &Target, data: &mut RunData) -> Outcome {
        let names: Vec<String> = t.members.iter().map(file_name_of).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let sets = first_volumes(&refs);
        if sets.is_empty() {
            return Outcome::Skipped("Not an archive".into());
        }
        for &i in sets.iter().flatten() {
            if !t.members[i].target_path.as_ref().is_some_and(|p| p.is_file()) {
                return Outcome::Failed(format!("File missing: {}", names[i]));
            }
        }
        let given = self.given.lock().unwrap().remove(&run_id);
        let saved = self.saved_passwords();
        let mut tries: Vec<String> = Vec::new();
        for p in given.iter().map(|(p, _)| p.clone()).chain([String::new()]).chain(saved.iter().cloned()) {
            if !tries.contains(&p) {
                tries.push(p);
            }
        }
        let out = match reserve_unique_dir(&t.dir, &t.name) {
            Ok(o) => o,
            Err(e) => return Outcome::Failed(format!("Couldn't make the folder: {e}")),
        };
        // Saved before extracting, so a restart after a crash knows what to clear away.
        data.extracted_to = Some(out.to_string_lossy().into_owned());
        self.save(run_id, RunState::Running, data);
        let tmp = tmp_of(&out);
        if tmp.exists() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        let undo = |data: &mut RunData| {
            let _ = std::fs::remove_dir_all(&tmp);
            remove_if_empty(&out);
            data.extracted_to = None;
        };
        let mut used: Vec<String> = Vec::new();
        for set in &sets {
            let first = t.members[set[0]].target_path.clone().unwrap_or_default();
            let order: Vec<String> = used.iter().chain(tries.iter().filter(|p| !used.contains(p))).cloned().collect();
            let mut ok = false;
            for pw in order {
                match self.deps.extractor.extract(&first, &tmp, &pw) {
                    ExtractOutcome::Ok => {
                        if !used.contains(&pw) {
                            used.push(pw);
                        }
                        ok = true;
                        break;
                    }
                    ExtractOutcome::WrongPassword => continue,
                    ExtractOutcome::Failed(msg) => {
                        undo(data);
                        return Outcome::Failed(format!("Archive is damaged: {msg}"));
                    }
                }
            }
            if !ok {
                undo(data);
                let msg = if given.is_some() { "Wrong password" } else { "Needs a password" };
                return Outcome::NeedsPassword(msg.into());
            }
        }
        if let Err(e) = std::fs::remove_dir(&out).and_then(|()| std::fs::rename(&tmp, &out)) {
            undo(data);
            return Outcome::Failed(format!("Couldn't finish extracting: {e}"));
        }
        if let Some((pw, true)) = &given
            && used.contains(pw)
            && !pw.is_empty()
            && !saved.contains(pw)
        {
            let saved = crate::secret::protect(pw.as_bytes()).and_then(|b| self.store.add_password(&b));
            if let Err(e) = saved {
                tracing::warn!(error = %e, "could not save the archive password");
            }
        }
        let inner_failed = self.extract_nested(&out, &used, &saved);
        if self.settings().recycle_archives {
            let volumes: Vec<PathBuf> =
                sets.iter().flatten().filter_map(|&i| t.members[i].target_path.clone()).collect();
            if let Err(e) = self.deps.recycler.recycle(&volumes) {
                tracing::warn!(error = %e, "could not recycle the extracted archives");
            }
        }
        let mut detail = format!("Extracted to {}", out.display());
        if inner_failed > 0 {
            let s = if inner_failed == 1 { "" } else { "s" };
            detail.push_str(&format!("; {inner_failed} inner archive{s} couldn't be extracted"));
        }
        Outcome::Done(detail)
    }

    /// Archives inside the output, extracted once each into a folder beside them. Returns how many failed.
    fn extract_nested(&self, out: &Path, used: &[String], saved: &[String]) -> usize {
        let mut by_dir: HashMap<PathBuf, Vec<String>> = HashMap::new();
        for f in walk_files(out) {
            if let (Some(dir), Some(name)) = (f.parent(), f.file_name()) {
                by_dir.entry(dir.to_path_buf()).or_default().push(name.to_string_lossy().into_owned());
            }
        }
        let empty = String::new();
        let mut tries: Vec<&String> = Vec::new();
        for p in used.iter().chain([&empty]).chain(saved.iter()) {
            if !tries.contains(&p) {
                tries.push(p);
            }
        }
        let mut dirs: Vec<_> = by_dir.into_iter().collect();
        dirs.sort();
        let mut failed = 0;
        for (dir, names) in dirs {
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            for set in first_volumes(&refs) {
                let first = dir.join(&names[set[0]]);
                let Ok(dest) = reserve_unique_dir(&dir, &base_name(&names[set[0]])) else {
                    failed += 1;
                    continue;
                };
                let ok = tries.iter().any(|pw| self.deps.extractor.extract(&first, &dest, pw) == ExtractOutcome::Ok);
                if !ok {
                    failed += 1;
                    let _ = std::fs::remove_dir_all(&dest);
                }
            }
        }
        failed
    }

    fn sort(&self, t: &mut Target, data: &mut RunData) -> Outcome {
        let settings = self.settings();
        let rules = self.rules().unwrap_or_default();
        let cats = self.categories();
        let date = chrono::Local::now().date_naive();
        let pkg_host = t.members.first().map(host_of).unwrap_or_default();
        let base = self.download_dir.read().unwrap_or_else(|e| e.into_inner()).clone();
        let package = t.name.clone();
        let place = |name: &str, size: Option<u64>, host: &str| -> Destination {
            let f = Facts { name, host, size, package: &package, date };
            decide(&f, &rules, &cats, settings.sort_categories, &base)
        };
        let mut dests: Vec<(String, PathBuf)> = Vec::new();
        data.moved.clear();

        // The extracted folder, placed by its largest file, keeps its own name.
        if let Some(out) = data.extracted_to.clone().map(PathBuf::from).filter(|p| p.is_dir()) {
            let largest = walk_files(&out)
                .into_iter()
                .filter_map(|p| std::fs::metadata(&p).ok().map(|m| (m.len(), p)))
                .max_by_key(|(len, _)| *len);
            if let Some((len, p)) = largest {
                let d = place(&p.file_name().unwrap_or_default().to_string_lossy(), Some(len), &pkg_host);
                if let Some(folder) = d.folder {
                    let name = out.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    match move_item(&out, &folder, &name) {
                        Ok(new) => {
                            data.extracted_to = Some(new.to_string_lossy().into_owned());
                            data.moved.push(new.to_string_lossy().into_owned());
                            dests.push((d.by, folder));
                        }
                        Err(e) => return Outcome::Failed(format!("Couldn't move {name}: {e}")),
                    }
                }
            }
        }

        // Plain files first, then archives, then checksum files (which follow the archives).
        let rank = |d: &Download| {
            let n = file_name_of(d);
            if is_checksum_file(&n) {
                2
            } else if is_archive_name(&n) {
                1
            } else {
                0
            }
        };
        let mut order: Vec<usize> = (0..t.members.len()).collect();
        order.sort_by_key(|&i| rank(&t.members[i]));
        let mut archive_folder: Option<(String, PathBuf)> = None;
        for i in order {
            let d = t.members[i].clone();
            let name = file_name_of(&d);
            let Some(src) = d.target_path.clone() else { continue };
            if !src.is_file() {
                if settings.recycle_archives && rank(&d) == 1 {
                    continue; // went to the Recycle Bin after extracting
                }
                return Outcome::Failed(format!("File missing: {name}"));
            }
            let dest = match (&archive_folder, rank(&d)) {
                (Some((by, folder)), 2) => {
                    Destination { folder: Some(folder.clone()), name: name.clone(), by: by.clone() }
                }
                _ => {
                    let size = std::fs::metadata(&src).ok().map(|m| m.len());
                    place(&name, size, &host_of(&d))
                }
            };
            let here = src.parent().map(Path::to_path_buf).unwrap_or_default();
            let folder = dest.folder.clone().unwrap_or_else(|| here.clone());
            if folder == here && dest.name == name {
                continue;
            }
            match move_item(&src, &folder, &dest.name) {
                Ok(new) => {
                    if let Err(e) = self.store.set_completed(d.id, &new) {
                        tracing::warn!(id = d.id, error = %e, "could not save the new path");
                    }
                    t.members[i].target_path = Some(new.clone());
                    data.moved.push(new.to_string_lossy().into_owned());
                    if rank(&d) == 1 && archive_folder.is_none() {
                        archive_folder = Some((dest.by.clone(), folder.clone()));
                    }
                    dests.push((dest.by, folder));
                }
                Err(e) => return Outcome::Failed(format!("Couldn't move {name}: {e}")),
            }
        }
        let mut folders: Vec<&PathBuf> = dests.iter().map(|(_, f)| f).collect();
        folders.sort();
        folders.dedup();
        match folders.len() {
            0 => Outcome::Skipped("Nothing to move".into()),
            1 => {
                let (by, folder) = &dests[0];
                if by.is_empty() {
                    Outcome::Done(format!("→ {}", folder.display()))
                } else {
                    Outcome::Done(format!("{by} → {}", folder.display()))
                }
            }
            n => Outcome::Done(format!("→ {n} folders")),
        }
    }

    fn scan(&self, t: &Target, data: &RunData) -> Outcome {
        if !self.settings().virus_scan {
            return Outcome::Skipped("Off".into());
        }
        let mut items: Vec<PathBuf> = data.moved.iter().map(PathBuf::from).collect();
        if items.is_empty() {
            items = t.members.iter().filter_map(|d| d.target_path.clone()).collect();
            items.extend(data.extracted_to.iter().map(PathBuf::from));
        }
        for p in items.iter().filter(|p| p.exists()) {
            match self.deps.scanner.scan(p) {
                ScanOutcome::Clean => {}
                ScanOutcome::Threat => return Outcome::Failed(format!("Threat found: {}", p.display())),
                ScanOutcome::Unavailable => return Outcome::Skipped("Windows Defender not found".into()),
            }
        }
        Outcome::Done("No threats".into())
    }

    fn notify(&self, t: &Target, data: &RunData) -> Outcome {
        let detail = |k| data.step(k).filter(|s| s.state == StepState::Done).and_then(|s| s.detail.clone());
        let body = detail(StepKind::Extract).or_else(|| detail(StepKind::Sort)).unwrap_or_default();
        let folder = data
            .extracted_to
            .as_ref()
            .map(PathBuf::from)
            .or_else(|| data.moved.first().and_then(|m| Path::new(m).parent().map(Path::to_path_buf)))
            .or_else(|| Some(t.dir.clone()));
        (self.deps.notify)(Notice { title: format!("{} is ready", t.name), body, folder });
        Outcome::Done("Sent".into())
    }
}
