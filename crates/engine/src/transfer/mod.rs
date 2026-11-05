//! Running one download: probe, plan ranges, run connections, save progress, finish.

pub(crate) mod conn;
pub(crate) mod state;

use crate::error::EngineError;
use crate::events::EventBus;
use crate::naming::{part_path, reserve_unique_path, sanitize_filename};
use crate::probe::probe;
use crate::ratelimit::{HostLimiter, RateLimiter};
use crate::segmenter::{MIB, Range, initial_plan};
use crate::store::Store;
use crate::types::{DownloadId, DownloadState, Event, ProbeInfo};
use conn::{ConnExit, Env, io_error};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use state::SegTable;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct TransferConfig {
    pub min_steal: u64,
    pub slow_ratio: f64,
    pub slow_window: Duration,
    pub save_interval: Duration,
    pub retry_delays: Vec<Duration>,
    pub max_attempts: u32,
    pub write_buffer: usize,
    pub progress_interval: Duration,
    /// Tests only: pretend this many bytes are free.
    pub free_space_override: Option<u64>,
}

impl TransferConfig {
    /// The spec's values.
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Self {
        TransferConfig {
            min_steal: MIB,
            slow_ratio: 0.25,
            slow_window: Duration::from_secs(10),
            save_interval: Duration::from_secs(2),
            retry_delays: [2, 4, 8, 16, 32, 60].into_iter().map(Duration::from_secs).collect(),
            max_attempts: 10,
            write_buffer: 256 * 1024,
            progress_interval: Duration::from_millis(250),
            free_space_override: None,
        }
    }

    /// Fast timings for tests: 10 ms retry delays, 300 ms slow window, 50 ms saves.
    pub fn for_tests() -> Self {
        TransferConfig {
            retry_delays: vec![Duration::from_millis(10); 6],
            slow_window: Duration::from_millis(300),
            save_interval: Duration::from_millis(50),
            ..Self::default()
        }
    }
}

impl Default for TransferConfig {
    fn default() -> Self {
        TransferConfig::default()
    }
}

#[derive(Clone)]
pub struct TransferCtx {
    pub client: reqwest::Client,
    pub store: Store,
    pub events: EventBus,
    pub global_limit: Arc<RateLimiter>,
    pub hosts: Arc<HostLimiter>,
    pub config: TransferConfig,
}

pub struct TransferJob {
    pub id: DownloadId,
    pub cancel: CancellationToken,
}

#[derive(Debug)]
pub enum TransferOutcome {
    Completed(PathBuf),
    Paused,
    Failed(EngineError),
}

/// The state a failed download is left in.
pub fn state_for_error(e: &EngineError) -> DownloadState {
    match e {
        EngineError::Offline => DownloadState::Offline,
        EngineError::LinkExpired(_) => DownloadState::LinkExpired,
        _ => DownloadState::Failed,
    }
}

/// Runs download `job.id` to completion, pause (cancel) or failure, and records the
/// final state in the store.
pub async fn run_transfer(job: TransferJob, ctx: &TransferCtx) -> TransferOutcome {
    let id = job.id;
    if let Err(e) = ctx.store.set_state(id, DownloadState::Running, None) {
        return TransferOutcome::Failed(e);
    }
    ctx.events.send(Event::StateChanged { id, state: DownloadState::Running, error: None });

    let out = match transfer(&job, ctx).await {
        Ok(Some(p)) => TransferOutcome::Completed(p),
        Ok(None) | Err(EngineError::Cancelled) => TransferOutcome::Paused,
        Err(e) => TransferOutcome::Failed(e),
    };
    let (state, error) = match &out {
        TransferOutcome::Completed(_) => (DownloadState::Completed, None),
        TransferOutcome::Paused => (DownloadState::Paused, None),
        TransferOutcome::Failed(e) => (state_for_error(e), Some(e.to_string())),
    };
    if !matches!(out, TransferOutcome::Completed(_)) {
        if let Err(e) = ctx.store.set_state(id, state, error.as_deref()) {
            tracing::warn!(id, error = %e, "could not record final state");
        }
    }
    ctx.events.send(Event::StateChanged { id, state, error });
    out
}

fn parse_headers(json: Option<&str>) -> HeaderMap {
    let mut h = HeaderMap::new();
    let Some(map) = json.and_then(|j| serde_json::from_str::<HashMap<String, String>>(j).ok()) else {
        return h;
    };
    for (k, v) in map {
        if let (Ok(k), Ok(v)) = (HeaderName::try_from(k.as_str()), HeaderValue::try_from(v.as_str())) {
            h.insert(k, v);
        }
    }
    h
}

/// Sleeps the retry delay for `attempt` (1-based); false if cancelled meanwhile.
async fn backoff(config: &TransferConfig, attempt: u32, cancel: &CancellationToken) -> bool {
    let d = config.retry_delays[(attempt as usize - 1).min(config.retry_delays.len() - 1)];
    tokio::select! {
        _ = cancel.cancelled() => false,
        _ = tokio::time::sleep(d) => true,
    }
}

async fn probe_with_retry(
    ctx: &TransferCtx,
    url: &str,
    headers: &HeaderMap,
    cancel: &CancellationToken,
) -> Result<ProbeInfo, EngineError> {
    let mut attempt = 0;
    loop {
        let res = tokio::select! {
            _ = cancel.cancelled() => return Err(EngineError::Cancelled),
            r = probe(&ctx.client, url, headers) => r,
        };
        match res {
            Err(e) if e.is_retryable() => {
                attempt += 1;
                if attempt >= ctx.config.max_attempts {
                    return Err(e);
                }
                if !backoff(&ctx.config, attempt, cancel).await {
                    return Err(EngineError::Cancelled);
                }
            }
            other => return other,
        }
    }
}

fn free_space(ctx: &TransferCtx, dir: &Path) -> Result<u64, EngineError> {
    match ctx.config.free_space_override {
        Some(v) => Ok(v),
        None => fs4::available_space(dir).map_err(EngineError::Io),
    }
}

/// Returns Ok(Some(path)) when complete, Ok(None) when paused.
async fn transfer(job: &TransferJob, ctx: &TransferCtx) -> Result<Option<PathBuf>, EngineError> {
    let id = job.id;
    let d = ctx.store.get_download(id)?;
    let headers = parse_headers(d.headers_json.as_deref());
    let host = url::Url::parse(&d.url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default();

    let fresh = probe_with_retry(ctx, &d.url, &headers, &job.cancel).await?;
    let file_name = match &d.file_name {
        Some(n) => sanitize_filename(n),
        None => {
            ctx.store.set_file_name(id, &fresh.file_name)?;
            fresh.file_name.clone()
        }
    };
    ctx.store.set_probe(id, &fresh)?;

    std::fs::create_dir_all(&d.target_dir)?;
    let part = part_path(&d.target_dir, &file_name, id);
    let size = fresh.size;
    let resumable = fresh.resumable && size.is_some();

    let ranges = match size {
        Some(s) if resumable => initial_plan(s, d.connections),
        _ => vec![Range { start: 0, pos: 0, end: size.unwrap_or(u64::MAX) }],
    };
    let written: u64 = ranges.iter().map(|r| r.pos - r.start).sum();
    if let Some(s) = size {
        if s - written > free_space(ctx, &d.target_dir)? {
            return Err(EngineError::DiskFull);
        }
    }

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&part)?;
    if let Some(s) = size {
        file.set_len(s).map_err(io_error)?;
    }
    let file = Arc::new(file);

    let conns = if resumable { (d.connections.clamp(1, 32) as usize).min(ranges.len()) } else { 1 };
    let env = Arc::new(Env {
        id,
        client: ctx.client.clone(),
        url: d.url.clone(),
        host,
        headers,
        file: file.clone(),
        table: Arc::new(Mutex::new(SegTable::new(ranges))),
        global: ctx.global_limit.clone(),
        local: Arc::new(RateLimiter::new(d.speed_limit)),
        hosts: ctx.hosts.clone(),
        events: ctx.events.clone(),
        config: ctx.config.clone(),
        resumable,
        cancel: job.cancel.child_token(),
        received: Arc::new(AtomicU64::new(0)),
        live: Arc::new(Mutex::new(HashMap::new())),
        next_live: AtomicU64::new(0),
    });

    let end = run_pass(ctx, &env, conns, size).await;
    let final_ranges = env.table.lock().unwrap().ranges();
    let total_written: u64 = final_ranges.iter().map(|r| r.pos - r.start).sum();
    drop(env);

    match end {
        PassEnd::Complete => {
            if size.is_none() {
                file.set_len(total_written).map_err(io_error)?;
            }
            file.sync_all().map_err(io_error)?;
            drop(file);
            let target = reserve_unique_path(&d.target_dir, &file_name)?;
            if let Err(e) = std::fs::rename(&part, &target) {
                let _ = std::fs::remove_file(&target);
                return Err(EngineError::Io(e));
            }
            ctx.store.set_completed(id, &target)?;
            ctx.store.save_segments(id, &[])?;
            Ok(Some(target))
        }
        PassEnd::Cancelled => {
            file.sync_all().map_err(io_error)?;
            ctx.store.save_segments(id, &final_ranges)?;
            Ok(None)
        }
        PassEnd::Failed(e) => {
            let _ = file.sync_all();
            ctx.store.save_segments(id, &final_ranges)?;
            Err(e)
        }
    }
}

enum PassEnd {
    Complete,
    Cancelled,
    Failed(EngineError),
}

/// Runs `conns` connections until the table is done, the job is cancelled or one fails.
/// Saves segments and emits progress on the way.
async fn run_pass(ctx: &TransferCtx, env: &Arc<Env>, conns: usize, size: Option<u64>) -> PassEnd {
    let mut set = JoinSet::new();
    for c in 0..conns {
        set.spawn(conn::worker(env.clone(), c));
    }
    let cfg = &ctx.config;
    let tick = cfg.save_interval.min(cfg.progress_interval).min(Duration::from_millis(50));
    let mut ticker = tokio::time::interval(tick);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_save = Instant::now();
    let mut last_progress: Option<Instant> = None;
    let mut speed_samples: VecDeque<(Instant, u64)> = VecDeque::new();
    let mut first_err: Option<ConnExit> = None;

    loop {
        tokio::select! {
            joined = set.join_next() => match joined {
                None => break,
                Some(Ok(Ok(()))) => {}
                Some(Ok(Err(exit))) => {
                    if first_err.is_none() {
                        first_err = Some(exit);
                        env.cancel.cancel();
                    }
                }
                Some(Err(join_err)) => {
                    if first_err.is_none() {
                        first_err = Some(ConnExit::Fatal(EngineError::Network(format!("connection task: {join_err}"))));
                        env.cancel.cancel();
                    }
                }
            },
            _ = ticker.tick() => {
                if last_save.elapsed() >= cfg.save_interval {
                    last_save = Instant::now();
                    let ranges = env.table.lock().unwrap().ranges();
                    if let Err(e) = ctx.store.save_segments(env.id, &ranges) {
                        tracing::warn!(id = env.id, error = %e, "saving segments failed");
                    }
                }
                if last_progress.is_none_or(|t| t.elapsed() >= cfg.progress_interval) {
                    last_progress = Some(Instant::now());
                    emit_progress(env, size, &mut speed_samples);
                }
            }
        }
    }

    match first_err {
        Some(ConnExit::Fatal(e)) => PassEnd::Failed(e),
        Some(ConnExit::RangeIgnored) => {
            PassEnd::Failed(EngineError::Network("server ignored the range request".into()))
        }
        None if env.cancel.is_cancelled() => PassEnd::Cancelled,
        None if env.table.lock().unwrap().all_done() => PassEnd::Complete,
        None => PassEnd::Failed(EngineError::Network("connections stopped before the end".into())),
    }
}

fn emit_progress(env: &Env, size: Option<u64>, samples: &mut VecDeque<(Instant, u64)>) {
    let now = Instant::now();
    let rx = env.received.load(Relaxed);
    samples.push_back((now, rx));
    while samples.len() > 2 && now.duration_since(samples[0].0) > Duration::from_secs(2) {
        samples.pop_front();
    }
    let (t0, b0) = samples[0];
    let dt = now.duration_since(t0).as_secs_f64();
    let speed_bps = if dt > 0.0 { ((rx - b0) as f64 / dt) as u64 } else { 0 };
    let (written, segments) = {
        let t = env.table.lock().unwrap();
        let segs = t.ranges().iter().map(|r| (r.start, r.pos, r.end)).collect();
        (t.written(), segs)
    };
    env.events.send(Event::Progress { id: env.id, written, size, speed_bps, segments });
}
