//! The download queue: starts queued downloads in priority order, up to a parallel limit.

use crate::error::EngineError;
use crate::naming::part_path;
use crate::schedule::{Schedule, is_within_window};
use crate::store::NewDownload;
use crate::transfer::{TransferCtx, TransferJob, run_transfer};
use crate::types::{DownloadId, DownloadState, Event, Priority};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

pub const MAX_PARALLEL: usize = 20;

struct Running {
    cancel: CancellationToken,
    removing: bool,
}

struct Inner {
    ctx: TransferCtx,
    max_parallel: AtomicUsize,
    running: Mutex<HashMap<DownloadId, Running>>,
    wake: Notify,
    stop: CancellationToken,
    /// Something ran since the last AllDone.
    ran: AtomicBool,
    schedule: Mutex<Option<Schedule>>,
    /// Downloads the schedule paused, to requeue when the window opens.
    auto_paused: Mutex<HashSet<DownloadId>>,
}

#[derive(Clone)]
pub struct Queue {
    inner: Arc<Inner>,
}

impl Queue {
    /// Spawns the dispatcher. Rows left `Running` by a crash go back to `Queued`.
    pub fn start(ctx: TransferCtx, max_parallel: usize) -> Queue {
        if let Ok(rows) = ctx.store.list_downloads() {
            for d in rows.into_iter().filter(|d| d.state == DownloadState::Running) {
                let _ = ctx.store.set_state(d.id, DownloadState::Queued, None);
            }
        }
        let q = Queue {
            inner: Arc::new(Inner {
                ctx,
                max_parallel: AtomicUsize::new(max_parallel.clamp(1, MAX_PARALLEL)),
                running: Mutex::new(HashMap::new()),
                wake: Notify::new(),
                stop: CancellationToken::new(),
                ran: AtomicBool::new(false),
                schedule: Mutex::new(None),
                auto_paused: Mutex::new(HashSet::new()),
            }),
        };
        let inner = q.inner.clone();
        tokio::spawn(async move {
            loop {
                dispatch(&inner);
                tokio::select! {
                    _ = inner.stop.cancelled() => break,
                    _ = inner.wake.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                }
            }
        });
        q
    }

    fn ctx(&self) -> &TransferCtx {
        &self.inner.ctx
    }

    fn set_state(&self, id: DownloadId, state: DownloadState, error: Option<&str>) {
        if self.ctx().store.set_state(id, state, error).is_ok() {
            self.ctx().events.send(Event::StateChanged { id, state, error: error.map(str::to_string) });
        }
    }

    pub fn add(&self, d: NewDownload) -> Result<DownloadId, EngineError> {
        let id = self.ctx().store.insert_download(&d)?;
        self.ctx().events.send(Event::StateChanged { id, state: DownloadState::Queued, error: None });
        self.inner.wake.notify_one();
        Ok(id)
    }

    pub fn pause(&self, id: DownloadId) {
        // Held throughout, so the dispatcher can't start this row between the check and the write.
        let running = self.inner.running.lock().unwrap();
        if let Some(r) = running.get(&id) {
            r.cancel.cancel();
            return;
        }
        if let Ok(d) = self.ctx().store.get_download(id)
            && d.state == DownloadState::Queued
        {
            self.set_state(id, DownloadState::Paused, None);
        }
    }

    /// Puts a paused or failed download back in the queue.
    pub fn resume(&self, id: DownloadId) {
        if self.inner.running.lock().unwrap().contains_key(&id) {
            return;
        }
        if let Ok(d) = self.ctx().store.get_download(id)
            && !matches!(d.state, DownloadState::Queued | DownloadState::Running | DownloadState::Completed)
        {
            self.set_state(id, DownloadState::Queued, None);
            self.inner.wake.notify_one();
        }
    }

    /// Stops it if running, then deletes the row and its part file. Finished files are never touched.
    pub fn remove(&self, id: DownloadId) {
        {
            // Held throughout, like `pause`, so a row being removed is never started.
            let mut running = self.inner.running.lock().unwrap();
            if let Some(r) = running.get_mut(&id) {
                r.removing = true;
                r.cancel.cancel();
                return;
            }
            delete_row_and_part(&self.inner.ctx, id);
        }
        self.inner.wake.notify_one();
    }

    pub fn set_priority(&self, id: DownloadId, p: Priority) {
        if let Ok(d) = self.ctx().store.get_download(id) {
            let _ = self.ctx().store.set_order(id, p, d.position);
            self.inner.wake.notify_one();
        }
    }

    pub fn move_to(&self, id: DownloadId, position: i64) {
        if let Ok(d) = self.ctx().store.get_download(id) {
            let _ = self.ctx().store.set_order(id, d.priority, position);
            self.inner.wake.notify_one();
        }
    }

    /// Moves `ids` to the front of their priority bands, in that order (see `Store::reorder`).
    pub fn reorder(&self, ids: &[DownloadId]) {
        let _ = self.ctx().store.reorder(ids);
        self.inner.wake.notify_one();
    }

    pub fn set_max_parallel(&self, n: usize) {
        self.inner.max_parallel.store(n.clamp(1, MAX_PARALLEL), SeqCst);
        self.inner.wake.notify_one();
    }

    pub fn max_parallel(&self) -> usize {
        self.inner.max_parallel.load(SeqCst)
    }

    pub fn set_global_limit(&self, bps: Option<u64>) {
        self.ctx().global_limit.set_rate(bps);
    }

    /// Outside the window nothing starts and running downloads pause; they resume
    /// when the window opens. `None` means no restriction.
    pub fn set_schedule(&self, s: Option<Schedule>) {
        *self.inner.schedule.lock().unwrap() = s;
        self.inner.wake.notify_one();
    }

    /// Pauses everything, saves, and stops the dispatcher. Used on app exit.
    pub async fn shutdown(&self) {
        self.inner.stop.cancel();
        for r in self.inner.running.lock().unwrap().values() {
            r.cancel.cancel();
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while !self.inner.running.lock().unwrap().is_empty() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

fn delete_row_and_part(ctx: &TransferCtx, id: DownloadId) {
    if let Ok(d) = ctx.store.get_download(id)
        && d.state != DownloadState::Completed
        && let Some(name) = &d.file_name
    {
        let part = part_path(&d.target_dir, name, id);
        if part.exists()
            && let Err(e) = std::fs::remove_file(&part)
        {
            tracing::warn!(id, error = %e, "could not delete part file");
        }
    }
    if ctx.store.delete_download(id).is_ok() {
        ctx.events.send(Event::Removed { id });
    }
}

/// Starts queued downloads while slots are free; emits AllDone once the queue drains.
fn dispatch(inner: &Arc<Inner>) {
    if inner.stop.is_cancelled() {
        return;
    }
    let allowed =
        inner.schedule.lock().unwrap().as_ref().is_none_or(|s| is_within_window(chrono::Local::now().naive_local(), s));
    if !allowed {
        let running = inner.running.lock().unwrap();
        let mut auto = inner.auto_paused.lock().unwrap();
        for (id, r) in running.iter() {
            if !r.removing && auto.insert(*id) {
                r.cancel.cancel();
            }
        }
        return;
    }
    let requeue: Vec<DownloadId> = inner.auto_paused.lock().unwrap().drain().collect();
    for id in requeue {
        if inner.ctx.store.get_download(id).is_ok_and(|d| d.state == DownloadState::Paused) {
            let _ = inner.ctx.store.set_state(id, DownloadState::Queued, None);
            inner.ctx.events.send(Event::StateChanged { id, state: DownloadState::Queued, error: None });
        }
    }

    let Ok(rows) = inner.ctx.store.list_downloads() else { return };
    let max = inner.max_parallel.load(SeqCst);
    let mut running = inner.running.lock().unwrap();
    for d in rows.iter().filter(|d| d.state == DownloadState::Queued) {
        if running.len() >= max {
            break;
        }
        // The snapshot may be stale: a pause or remove may have landed since it was read.
        if running.contains_key(&d.id) || !inner.ctx.store.start_if_queued(d.id).unwrap_or(false) {
            continue;
        }
        let cancel = CancellationToken::new();
        running.insert(d.id, Running { cancel: cancel.clone(), removing: false });
        inner.ran.store(true, SeqCst);
        let inner2 = inner.clone();
        let id = d.id;
        tokio::spawn(async move {
            run_transfer(TransferJob { id, cancel }, &inner2.ctx).await;
            let removing = inner2.running.lock().unwrap().remove(&id).is_some_and(|r| r.removing);
            if removing {
                delete_row_and_part(&inner2.ctx, id);
            }
            inner2.wake.notify_one();
        });
    }
    let busy = !running.is_empty()
        || rows.iter().any(|d| d.state == DownloadState::Queued || d.state == DownloadState::Running);
    if !busy && inner.ran.swap(false, SeqCst) {
        inner.ctx.events.send(Event::AllDone);
    }
}
