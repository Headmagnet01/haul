//! Running one download: probe, plan ranges, run connections, save progress, finish.

pub(crate) mod conn;
pub(crate) mod state;

use crate::error::EngineError;
use crate::events::EventBus;
use crate::naming::{part_path, reserve_unique_path, sanitize_filename};
use crate::probe::probe;
use crate::ratelimit::{HostLimiter, RateLimiter};
use crate::segmenter::{BYTES_PER_INITIAL_CONN, MIB, Range, initial_plan};
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
        // Not a failure: the user frees space and resumes.
        EngineError::DiskFull => DownloadState::Paused,
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
    if !matches!(out, TransferOutcome::Completed(_))
        && let Err(e) = ctx.store.set_state(id, state, error.as_deref())
    {
        tracing::warn!(id, error = %e, "could not record final state");
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

/// Why saved progress can't be trusted against a fresh probe, checked in the
/// spec's order: ETag, then Last-Modified, then size.
fn fingerprint_mismatch(old: &ProbeInfo, new: &ProbeInfo) -> Option<&'static str> {
    if old.etag != new.etag {
        Some("etag changed")
    } else if old.last_modified != new.last_modified {
        Some("last-modified changed")
    } else if old.size != new.size {
        Some("size changed")
    } else {
        None
    }
}

/// A URL for the Mark of the Web: no `user:password`.
fn without_credentials(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut u) => {
            let _ = u.set_username("");
            let _ = u.set_password(None);
            u.to_string()
        }
        Err(_) => url.to_string(),
    }
}

/// Marks a finished file as from the internet (the `Zone.Identifier` stream browsers write), so
/// Windows SmartScreen and Office's Protected View check it on open. A drive without streams
/// (FAT32, exFAT) can't hold the mark; that is logged, not an error.
fn mark_of_the_web(file: &Path, url: &str, referrer: Option<&str>) {
    let mut text = String::from(
        "[ZoneTransfer]
ZoneId=3
",
    );
    if let Some(r) = referrer {
        text.push_str(&format!(
            "ReferrerUrl={}
",
            without_credentials(r)
        ));
    }
    text.push_str(&format!(
        "HostUrl={}
",
        without_credentials(url)
    ));
    let mut stream = file.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    if let Err(e) = std::fs::write(&stream, text) {
        tracing::warn!(file = %file.display(), error = %e, "could not write the Mark of the Web");
    }
}

/// A download folder Haul can't make or write in (a removed drive, a read-only folder).
fn folder_unavailable(dir: &Path, e: std::io::Error) -> EngineError {
    tracing::warn!(dir = %dir.display(), error = %e, "download folder unavailable");
    EngineError::Invalid(format!("Download folder {} isn't available. Change it in Settings.", dir.display()))
}

/// Returns Ok(Some(path)) when complete, Ok(None) when paused.
async fn transfer(job: &TransferJob, ctx: &TransferCtx) -> Result<Option<PathBuf>, EngineError> {
    let id = job.id;
    let d = ctx.store.get_download(id)?;
    let headers = parse_headers(d.headers_json.as_deref());
    let host = url::Url::parse(&d.url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default();

    let mut old_probe = d.probe.clone();
    let mut file_name = d.file_name.as_deref().map(sanitize_filename);
    let mut allow_resume = true;
    let mut force_sequential = false;
    let mut restarted_416 = false;

    loop {
        let fresh = probe_with_retry(ctx, &d.url, &headers, &job.cancel).await?;
        let name = match &file_name {
            Some(n) => n.clone(),
            None => {
                ctx.store.set_file_name(id, &fresh.file_name)?;
                file_name = Some(fresh.file_name.clone());
                fresh.file_name.clone()
            }
        };
        std::fs::create_dir_all(&d.target_dir).map_err(|e| folder_unavailable(&d.target_dir, e))?;
        let part = part_path(&d.target_dir, &name, id);
        let size = fresh.size;
        let resumable = fresh.resumable && size.is_some() && !force_sequential;

        let saved = ctx.store.load_segments(id)?;
        let resume = allow_resume && resumable && !saved.is_empty() && {
            let why = if !part.exists() {
                Some("part file missing")
            } else if !state::ranges_valid(&saved, size.unwrap_or(0)) {
                Some("saved ranges invalid")
            } else {
                old_probe.as_ref().map_or(Some("no stored probe"), |old| fingerprint_mismatch(old, &fresh))
            };
            if let Some(why) = why {
                tracing::info!(id, why, "restarting from zero");
            }
            why.is_none()
        };
        // Forget stale progress before storing the new fingerprint, so an exit from
        // here on can never pair old bytes with the new file's probe.
        if !resume && !saved.is_empty() {
            ctx.store.save_segments(id, &[])?;
        }
        ctx.store.set_probe(id, &fresh)?;
        old_probe = Some(fresh.clone());

        let ranges = match size {
            _ if resume => saved,
            Some(s) if resumable => initial_plan(s, d.connections),
            _ => vec![Range { start: 0, pos: 0, end: size.unwrap_or(u64::MAX) }],
        };
        // The part file is sized up front, so only the growth past its current length is new space.
        let allocated = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if let Some(s) = size
            && s.saturating_sub(allocated) > free_space(ctx, &d.target_dir)?
        {
            return Err(EngineError::DiskFull);
        }

        let file =
            std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(!resume).open(&part).map_err(
                |e| match e.kind() {
                    std::io::ErrorKind::PermissionDenied => folder_unavailable(&d.target_dir, e),
                    _ => EngineError::Io(e),
                },
            )?;
        if let Some(s) = size {
            file.set_len(s).map_err(io_error)?;
        }
        let file = Arc::new(file);

        // As many connections as a fresh plan of the remaining bytes would get (the extras steal),
        // but never fewer than the unfinished ranges.
        let unfinished = ranges.iter().filter(|r| !r.is_done()).count();
        let remaining: u64 = ranges.iter().map(|r| r.remaining()).sum();
        let by_size = (remaining / BYTES_PER_INITIAL_CONN) as usize;
        let conns =
            if resumable { (d.connections.clamp(1, 32) as usize).min(unfinished.max(by_size)).max(1) } else { 1 };
        let env = Arc::new(Env {
            id,
            client: ctx.client.clone(),
            url: d.url.clone(),
            host: host.clone(),
            headers: headers.clone(),
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
            active: std::sync::atomic::AtomicUsize::new(conns),
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
                let target = reserve_unique_path(&d.target_dir, &name)?;
                if let Err(e) = std::fs::rename(&part, &target) {
                    let _ = std::fs::remove_file(&target);
                    return Err(EngineError::Io(e));
                }
                let referrer = headers.get(reqwest::header::REFERER).and_then(|v| v.to_str().ok());
                mark_of_the_web(&target, &d.url, referrer);
                ctx.store.set_completed(id, &target)?;
                ctx.store.save_segments(id, &[])?;
                return Ok(Some(target));
            }
            PassEnd::Cancelled => {
                file.sync_all().map_err(io_error)?;
                ctx.store.save_segments(id, if resumable { &final_ranges } else { &[] })?;
                return Ok(None);
            }
            PassEnd::RangeIgnored => {
                tracing::info!(id, "server ignored a range request; one connection from zero");
                force_sequential = true;
                allow_resume = false;
                ctx.store.save_segments(id, &[])?;
            }
            PassEnd::Failed(EngineError::RangeNotSatisfiable) if !restarted_416 => {
                tracing::info!(id, "416; restarting from zero once");
                restarted_416 = true;
                allow_resume = false;
                ctx.store.save_segments(id, &[])?;
            }
            PassEnd::Failed(e) => {
                let _ = file.sync_all();
                ctx.store.save_segments(id, if resumable { &final_ranges } else { &[] })?;
                return Err(e);
            }
        }
    }
}

enum PassEnd {
    Complete,
    Cancelled,
    RangeIgnored,
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
    let mut slow_hist: HashMap<u64, VecDeque<(Instant, u64)>> = HashMap::new();

    loop {
        tokio::select! {
            joined = set.join_next() => match joined {
                // Everyone left cleanly but a range is still open: a connection shed itself
                // (429) while the last peer was already on its way out. Start a fresh one.
                None if first_err.is_none() && !env.cancel.is_cancelled() && !env.table.lock().unwrap().all_done() => {
                    tracing::debug!(id = env.id, "all connections left early; starting one more");
                    env.active.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    set.spawn(conn::worker(env.clone(), conns));
                }
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
                if env.resumable {
                    kill_slow_connections(env, cfg, &mut slow_hist);
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
        Some(ConnExit::RangeIgnored) => PassEnd::RangeIgnored,
        None if env.cancel.is_cancelled() => PassEnd::Cancelled,
        None if env.table.lock().unwrap().all_done() => PassEnd::Complete,
        None => PassEnd::Failed(EngineError::Network("connections stopped before the end".into())),
    }
}

/// Measures each live response over `slow_window` and cancels the ones running below
/// `slow_ratio` x the median, or that received nothing at all. Their ranges stay in the
/// table for a fresh connection.
fn kill_slow_connections(env: &Env, cfg: &TransferConfig, hist: &mut HashMap<u64, VecDeque<(Instant, u64)>>) {
    let now = Instant::now();
    let window = cfg.slow_window;
    let live = env.live.lock().unwrap();
    hist.retain(|k, _| live.contains_key(k));
    let mut speeds: Vec<(u64, f64)> = Vec::new();
    for (&k, l) in live.iter() {
        let h = hist.entry(k).or_default();
        h.push_back((now, l.received.load(Relaxed)));
        while h.len() > 2 && now.duration_since(h[1].0) >= window {
            h.pop_front();
        }
        let (t0, b0) = h[0];
        let (t1, b1) = *h.back().unwrap();
        let dt = t1.duration_since(t0);
        if now.duration_since(l.since) >= window && dt >= window.mul_f64(0.9) {
            speeds.push((k, (b1 - b0) as f64 / dt.as_secs_f64()));
        }
    }
    // A response that delivered nothing for a whole window is stalled, even when every
    // connection is (so there's no healthy median to compare against). Under a speed limit
    // a connection can legitimately sit out a window waiting for tokens, so skip it then.
    let limited = env.global.rate().is_some() || env.local.rate().is_some();
    if !limited {
        speeds.retain(|&(k, bps)| {
            if bps > 0.0 {
                return true;
            }
            tracing::debug!(id = env.id, "replacing stalled connection");
            live[&k].kill.cancel();
            hist.remove(&k);
            false
        });
    }
    if speeds.len() < 2 {
        return;
    }
    let mut sorted: Vec<f64> = speeds.iter().map(|s| s.1).collect();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    for (k, bps) in speeds {
        if crate::segmenter::is_slow(bps, median, cfg.slow_ratio) {
            tracing::debug!(id = env.id, bps, median, "replacing slow connection");
            live[&k].kill.cancel();
            hist.remove(&k);
        }
    }
}

/// How far back the reported speed averages. Long enough that a line's normal dips don't jump around.
pub const SPEED_WINDOW: Duration = Duration::from_secs(5);

/// Records `(now, rx)` and returns the average bytes per second over the last `window`.
fn windowed_speed(samples: &mut VecDeque<(Instant, u64)>, now: Instant, rx: u64, window: Duration) -> u64 {
    samples.push_back((now, rx));
    while samples.len() > 2 && now.duration_since(samples[0].0) > window {
        samples.pop_front();
    }
    let (t0, b0) = samples[0];
    let dt = now.duration_since(t0).as_secs_f64();
    if dt > 0.0 { (rx.saturating_sub(b0) as f64 / dt) as u64 } else { 0 }
}

fn emit_progress(env: &Env, size: Option<u64>, samples: &mut VecDeque<(Instant, u64)>) {
    let rx = env.received.load(Relaxed);
    let speed_bps = windowed_speed(samples, Instant::now(), rx, SPEED_WINDOW);
    let (written, segments) = {
        let t = env.table.lock().unwrap();
        let segs = t.ranges().iter().map(|r| (r.start, r.pos, r.end)).collect();
        (t.written(), segs)
    };
    env.events.send(Event::Progress { id: env.id, written, size, speed_bps, segments });
}

#[cfg(test)]
mod speed_tests {
    use super::*;

    #[test]
    fn speed_uses_five_second_window() {
        let t0 = Instant::now();
        let mut samples = VecDeque::new();
        let mut speed = 0;
        // 1 MB/s for 4 s, sampled every 250 ms, then nothing for 2 s.
        for i in 0..=24u64 {
            let rx = (i.min(16)) * 250_000;
            speed = windowed_speed(&mut samples, t0 + Duration::from_millis(i * 250), rx, SPEED_WINDOW);
        }
        assert!(speed > 0, "a 2 s window would report 0 here");
        assert_eq!(SPEED_WINDOW, Duration::from_secs(5));
    }
}
