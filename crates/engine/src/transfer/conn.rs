//! One connection's loop: claim or steal a range, fetch it, write it, repeat.

use super::state::SegTable;
use crate::error::EngineError;
use crate::events::EventBus;
use crate::ratelimit::{HostLimiter, RateLimiter};
use crate::transfer::TransferConfig;
use crate::types::{DownloadId, Event};
use futures::StreamExt;
use reqwest::header::{self, HeaderMap};
use reqwest::{Client, StatusCode};
use std::collections::HashMap;
use std::fs::File;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

/// Why a connection stopped for good.
#[derive(Debug)]
pub(crate) enum ConnExit {
    /// A fatal error for the whole transfer.
    Fatal(EngineError),
    /// The server answered a ranged request with the whole file.
    RangeIgnored,
}

/// One live HTTP response, as seen by the slow-connection monitor.
pub(crate) struct Live {
    pub kill: CancellationToken,
    pub received: Arc<AtomicU64>,
    pub since: Instant,
}

/// Everything a connection needs. Shared by all connections of one run.
pub(crate) struct Env {
    pub id: DownloadId,
    pub client: Client,
    pub url: String,
    pub host: String,
    pub headers: HeaderMap,
    pub file: Arc<File>,
    pub table: Arc<Mutex<SegTable>>,
    pub global: Arc<RateLimiter>,
    pub local: Arc<RateLimiter>,
    pub hosts: Arc<HostLimiter>,
    pub events: EventBus,
    pub config: TransferConfig,
    /// Ranged requests allowed. Off = one sequential stream from 0.
    pub resumable: bool,
    pub cancel: CancellationToken,
    /// Total bytes received by all connections (for the speed figure).
    pub received: Arc<AtomicU64>,
    pub live: Arc<Mutex<HashMap<u64, Live>>>,
    pub next_live: AtomicU64,
    /// Connections still running in this pass (set to the spawn count).
    pub active: AtomicUsize,
}

enum FetchEnd {
    Done,
    Cancelled,
    Killed,
}

/// Writes `buf` at `offset` from a blocking thread.
async fn write_at(file: Arc<File>, offset: u64, buf: Vec<u8>) -> std::io::Result<Vec<u8>> {
    tokio::task::spawn_blocking(move || {
        let mut done = 0;
        while done < buf.len() {
            #[cfg(windows)]
            let n = std::os::windows::fs::FileExt::seek_write(&*file, &buf[done..], offset + done as u64)?;
            #[cfg(unix)]
            let n = std::os::unix::fs::FileExt::write_at(&*file, &buf[done..], offset + done as u64)?;
            if n == 0 {
                return Err(std::io::Error::new(std::io::ErrorKind::WriteZero, "wrote 0 bytes"));
            }
            done += n;
        }
        Ok(buf)
    })
    .await
    .map_err(std::io::Error::other)?
}

pub(crate) fn io_error(e: std::io::Error) -> EngineError {
    // ERROR_HANDLE_DISK_FULL (39), ERROR_DISK_FULL (112); ENOSPC (28) on unix.
    match e.raw_os_error() {
        #[cfg(windows)]
        Some(39) | Some(112) => EngineError::DiskFull,
        #[cfg(unix)]
        Some(28) => EngineError::DiskFull,
        _ => EngineError::Io(e),
    }
}

/// The connection loop. Returns Ok when there is nothing left to take or the job was cancelled.
pub(crate) async fn worker(env: Arc<Env>, conn: usize) -> Result<(), ConnExit> {
    struct Active<'a>(&'a AtomicUsize, bool);
    impl Drop for Active<'_> {
        fn drop(&mut self) {
            if !self.1 {
                self.0.fetch_sub(1, SeqCst);
            }
        }
    }
    let mut active = Active(&env.active, false);
    loop {
        if env.cancel.is_cancelled() {
            return Ok(());
        }
        let idx = {
            let mut t = env.table.lock().unwrap();
            match t.claim(conn) {
                Some(i) => Some(i),
                None if env.resumable => t.steal(conn, env.config.min_steal).map(|(i, bytes)| {
                    env.events.send(Event::Steal { id: env.id, bytes });
                    i
                }),
                None => None,
            }
        };
        let Some(idx) = idx else { return Ok(()) };

        let mut attempts: u32 = 0;
        let end = loop {
            let before = env.table.lock().unwrap().slots[idx].range.pos;
            let res = fetch(&env, idx).await;
            let progressed = env.table.lock().unwrap().slots[idx].range.pos > before;
            match res {
                Ok(e) => break e,
                Err(ConnExit::Fatal(EngineError::Http(429)))
                    if env.active.try_update(SeqCst, SeqCst, |n| (n > 1).then(|| n - 1)).is_ok() =>
                {
                    // Too many connections for this host: drop this one; the others
                    // pick up its range by stealing. The last connection never sheds.
                    tracing::debug!(id = env.id, conn, "429: shedding a connection");
                    active.1 = true;
                    env.table.lock().unwrap().release(idx);
                    return Ok(());
                }
                Err(ConnExit::Fatal(e)) if e.is_retryable() => {
                    if progressed {
                        attempts = 0;
                    }
                    attempts += 1;
                    if attempts >= env.config.max_attempts {
                        env.table.lock().unwrap().release(idx);
                        return Err(ConnExit::Fatal(e));
                    }
                    let delays = &env.config.retry_delays;
                    let d = delays[(attempts as usize - 1).min(delays.len() - 1)];
                    tracing::debug!(id = env.id, conn, attempts, error = %e, "retrying range");
                    tokio::select! {
                        _ = env.cancel.cancelled() => break FetchEnd::Cancelled,
                        _ = tokio::time::sleep(d) => {}
                    }
                }
                Err(other) => {
                    env.table.lock().unwrap().release(idx);
                    return Err(other);
                }
            }
        };
        env.table.lock().unwrap().release(idx);
        match end {
            FetchEnd::Cancelled => return Ok(()),
            FetchEnd::Done | FetchEnd::Killed => continue,
        }
    }
}

/// One HTTP request for slot `idx`, streamed into the file.
async fn fetch(env: &Env, idx: usize) -> Result<FetchEnd, ConnExit> {
    let (mut pos, end) = {
        let mut t = env.table.lock().unwrap();
        let r = &mut t.slots[idx].range;
        if !env.resumable {
            r.pos = 0; // can't resume: start the stream over
        }
        (r.pos, r.end)
    };
    if pos >= end {
        return Ok(FetchEnd::Done);
    }

    let _permit = tokio::select! {
        _ = env.cancel.cancelled() => return Ok(FetchEnd::Cancelled),
        p = env.hosts.acquire(&env.host) => p,
    };
    let mut req = env.client.get(&env.url).headers(env.headers.clone());
    if env.resumable {
        let last = if end == u64::MAX { String::new() } else { (end - 1).to_string() };
        req = req.header(header::RANGE, format!("bytes={pos}-{last}"));
    }
    let resp = tokio::select! {
        _ = env.cancel.cancelled() => return Ok(FetchEnd::Cancelled),
        r = req.send() => r.map_err(|e| ConnExit::Fatal(e.into()))?,
    };
    match resp.status() {
        StatusCode::PARTIAL_CONTENT => {
            let start = resp
                .headers()
                .get(header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("bytes "))
                .and_then(|v| v.split('-').next())
                .and_then(|v| v.trim().parse::<u64>().ok());
            if start != Some(pos) {
                return Err(ConnExit::RangeIgnored);
            }
        }
        StatusCode::OK if env.resumable && pos > 0 => return Err(ConnExit::RangeIgnored),
        s if s.is_success() => {}
        s => return Err(ConnExit::Fatal(EngineError::from_status(s.as_u16()))),
    }

    let live_id = env.next_live.fetch_add(1, Relaxed);
    let kill = CancellationToken::new();
    let received = Arc::new(AtomicU64::new(0));
    env.live
        .lock()
        .unwrap()
        .insert(live_id, Live { kill: kill.clone(), received: received.clone(), since: Instant::now() });
    struct Unregister<'a>(&'a Env, u64);
    impl Drop for Unregister<'_> {
        fn drop(&mut self) {
            self.0.live.lock().unwrap().remove(&self.1);
        }
    }
    let _unregister = Unregister(env, live_id);

    let cap = env.config.write_buffer.max(16 * 1024);
    let mut buf: Vec<u8> = Vec::with_capacity(cap);
    let mut stream = resp.bytes_stream();

    // Writes the buffer and moves the slot's pos (only after the bytes reached the OS).
    async fn flush(env: &Env, idx: usize, pos: &mut u64, buf: &mut Vec<u8>) -> Result<(), ConnExit> {
        if buf.is_empty() {
            return Ok(());
        }
        let data = std::mem::take(buf);
        let n = data.len() as u64;
        let mut data = write_at(env.file.clone(), *pos, data).await.map_err(|e| ConnExit::Fatal(io_error(e)))?;
        *pos += n;
        {
            let mut t = env.table.lock().unwrap();
            let r = &mut t.slots[idx].range;
            r.pos = (*pos).min(r.end);
        }
        data.clear();
        *buf = data;
        Ok(())
    }

    loop {
        let next = tokio::select! {
            biased;
            _ = env.cancel.cancelled() => {
                flush(env, idx, &mut pos, &mut buf).await?;
                return Ok(FetchEnd::Cancelled);
            }
            _ = kill.cancelled() => {
                flush(env, idx, &mut pos, &mut buf).await?;
                return Ok(FetchEnd::Killed);
            }
            n = stream.next() => n,
        };
        match next {
            None => {
                flush(env, idx, &mut pos, &mut buf).await?;
                let mut t = env.table.lock().unwrap();
                let r = &mut t.slots[idx].range;
                if r.end == u64::MAX {
                    r.end = pos; // unknown size: EOF is the end
                    return Ok(FetchEnd::Done);
                }
                if r.pos < r.end {
                    return Err(ConnExit::Fatal(EngineError::Network("connection closed early".into())));
                }
                return Ok(FetchEnd::Done);
            }
            Some(Err(e)) => {
                flush(env, idx, &mut pos, &mut buf).await?;
                return Err(ConnExit::Fatal(EngineError::Network(e.to_string())));
            }
            Some(Ok(bytes)) => {
                let cur_end = env.table.lock().unwrap().slots[idx].range.end;
                let room = cur_end.saturating_sub(pos + buf.len() as u64);
                let take = (bytes.len() as u64).min(room) as usize;
                if take > 0 {
                    env.global.acquire(take as u64).await;
                    env.local.acquire(take as u64).await;
                    buf.extend_from_slice(&bytes[..take]);
                    received.fetch_add(take as u64, Relaxed);
                    env.received.fetch_add(take as u64, Relaxed);
                }
                let reached_end = pos + buf.len() as u64 >= cur_end;
                if buf.len() >= cap || reached_end {
                    flush(env, idx, &mut pos, &mut buf).await?;
                }
                if reached_end {
                    return Ok(FetchEnd::Done);
                }
            }
        }
    }
}
