//! A local HTTP server for engine tests. It can throttle, drop connections,
//! refuse or ignore ranges, hide the length and redirect.
#![allow(dead_code)]

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

/// Deterministic test content: byte i = ((i * 31 + 7) % 251).
pub fn body(len: usize) -> Bytes {
    (0..len).map(|i| ((i * 31 + 7) % 251) as u8).collect::<Vec<u8>>().into()
}

/// Panics unless the file at `path` equals `body(len)` (compared by SHA-256).
pub fn assert_file_eq(path: &Path, len: usize) {
    let got = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_eq!(got.len(), len, "length of {}", path.display());
    assert!(Sha256::digest(&got) == Sha256::digest(body(len)), "content of {} differs", path.display());
}

#[derive(Clone, Default, Debug)]
pub struct Spec {
    pub len: usize,
    pub ranges: bool,
    pub head_allowed: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_disposition: Option<String>,
    /// Force this status on every request.
    pub status: Option<u16>,
    /// Throttle each response to this rate.
    pub bytes_per_sec: Option<u64>,
    /// (offset, bps): responses starting at >= offset use this rate instead.
    pub slow_from: Option<(u64, u64)>,
    /// The first response for each range start aborts after N bytes.
    pub drop_after: Option<u64>,
    /// The first response for each range start sends N bytes, then hangs without closing.
    pub stall_after: Option<u64>,
    /// After N requests, answer ranged GETs with 200 and the full body.
    pub ignore_range_after_requests: Option<u32>,
    /// Omit Content-Length and stream chunked.
    pub no_length: bool,
    /// This path answers 302 -> /file.
    pub redirect_from: Option<String>,
    /// Answer every ranged GET with this status instead.
    pub range_status: Option<u16>,
    /// GETs beyond this many open responses get 429 Too Many Requests.
    pub max_open: Option<u32>,
}

impl Spec {
    pub fn new(len: usize) -> Self {
        Spec { len, ranges: true, head_allowed: true, ..Default::default() }
    }
}

pub struct TestServer {
    pub url: String,
    pub base: String,
    pub requests: Arc<AtomicU32>,
    pub bytes_sent: Arc<AtomicU64>,
    pub max_concurrent: Arc<AtomicU32>,
    pub spec: Arc<Mutex<Spec>>,
    /// How many 429s were sent.
    pub too_many: Arc<AtomicU32>,
}

struct Srv {
    spec: Arc<Mutex<Spec>>,
    data: Mutex<Bytes>,
    requests: Arc<AtomicU32>,
    bytes_sent: Arc<AtomicU64>,
    in_flight: Arc<AtomicU32>,
    max_concurrent: Arc<AtomicU32>,
    too_many: Arc<AtomicU32>,
    dropped_starts: Mutex<HashSet<u64>>,
    stalled_starts: Mutex<HashSet<u64>>,
}

struct InFlight {
    now: Arc<AtomicU32>,
}
impl InFlight {
    fn new(now: Arc<AtomicU32>, max: &AtomicU32) -> Self {
        let n = now.fetch_add(1, SeqCst) + 1;
        max.fetch_max(n, SeqCst);
        InFlight { now }
    }
}
impl Drop for InFlight {
    fn drop(&mut self) {
        self.now.fetch_sub(1, SeqCst);
    }
}

pub async fn serve(spec: Spec) -> TestServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let redirect_from = spec.redirect_from.clone();
    let srv = Arc::new(Srv {
        data: Mutex::new(body(spec.len)),
        spec: Arc::new(Mutex::new(spec)),
        requests: Arc::new(AtomicU32::new(0)),
        bytes_sent: Arc::new(AtomicU64::new(0)),
        in_flight: Arc::new(AtomicU32::new(0)),
        max_concurrent: Arc::new(AtomicU32::new(0)),
        too_many: Arc::new(AtomicU32::new(0)),
        dropped_starts: Mutex::new(HashSet::new()),
        stalled_starts: Mutex::new(HashSet::new()),
    });
    let mut app = Router::new().route("/file", get(handle));
    if let Some(p) = &redirect_from {
        app = app.route(p, get(redirect));
    }
    let app = app.with_state(srv.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    TestServer {
        url: format!("http://{addr}/file"),
        base: format!("http://{addr}"),
        requests: srv.requests.clone(),
        bytes_sent: srv.bytes_sent.clone(),
        max_concurrent: srv.max_concurrent.clone(),
        spec: srv.spec.clone(),
        too_many: srv.too_many.clone(),
    }
}

async fn redirect(State(s): State<Arc<Srv>>) -> Response {
    s.requests.fetch_add(1, SeqCst);
    (StatusCode::FOUND, [(header::LOCATION, "/file")]).into_response()
}

/// Parses `bytes=a-b` / `bytes=a-`. Ok(None) = not a usable range header,
/// Err(()) = unsatisfiable.
fn parse_range(v: &str, len: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(spec) = v.strip_prefix("bytes=") else { return Ok(None) };
    let Some((a, b)) = spec.split_once('-') else { return Ok(None) };
    let Ok(start) = a.trim().parse::<u64>() else { return Ok(None) };
    if start >= len {
        return Err(());
    }
    let end = match b.trim() {
        "" => len - 1,
        b => match b.parse::<u64>() {
            Ok(e) if e >= start => e.min(len - 1),
            _ => return Ok(None),
        },
    };
    Ok(Some((start, end)))
}

struct Stream {
    data: Bytes,
    pos: u64,
    end: u64,
    sent: u64,
    bps: Option<u64>,
    drop_at: Option<u64>,
    stall_at: Option<u64>,
    t0: Instant,
    bytes_sent: Arc<AtomicU64>,
    _guard: InFlight,
}

async fn handle(State(s): State<Arc<Srv>>, method: Method, headers: HeaderMap) -> Response {
    let n = s.requests.fetch_add(1, SeqCst) + 1;
    let spec = s.spec.lock().unwrap().clone();
    if let Some(code) = spec.status {
        return StatusCode::from_u16(code).unwrap().into_response();
    }
    let is_head = method == Method::HEAD;
    if is_head && !spec.head_allowed {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let data = {
        let mut d = s.data.lock().unwrap();
        if d.len() != spec.len {
            *d = body(spec.len);
        }
        d.clone()
    };
    let len = spec.len as u64;

    let mut range = None;
    if spec.ranges
        && !is_head
        && let Some(v) = headers.get(header::RANGE).and_then(|v| v.to_str().ok())
    {
        if let Some(code) = spec.range_status {
            return StatusCode::from_u16(code).unwrap().into_response();
        }
        let ignore = spec.ignore_range_after_requests.is_some_and(|k| n > k);
        if !ignore {
            match parse_range(v, len) {
                Ok(r) => range = r,
                Err(()) => {
                    return (StatusCode::RANGE_NOT_SATISFIABLE, [(header::CONTENT_RANGE, format!("bytes */{len}"))])
                        .into_response();
                }
            }
        }
    }

    let mut b = Response::builder();
    if spec.ranges {
        b = b.header(header::ACCEPT_RANGES, "bytes");
    }
    if let Some(e) = &spec.etag {
        b = b.header(header::ETAG, format!("\"{e}\""));
    }
    if let Some(lm) = &spec.last_modified {
        b = b.header(header::LAST_MODIFIED, lm.as_str());
    }
    if let Some(cd) = &spec.content_disposition {
        b = b.header(header::CONTENT_DISPOSITION, cd.as_str());
    }
    let (status, start, end) = match range {
        Some((a, z)) => (StatusCode::PARTIAL_CONTENT, a, z + 1),
        None => (StatusCode::OK, 0, len),
    };
    if status == StatusCode::PARTIAL_CONTENT {
        b = b.header(header::CONTENT_RANGE, format!("bytes {start}-{}/{len}", end - 1));
    }
    if !spec.no_length {
        b = b.header(header::CONTENT_LENGTH, end - start);
    }
    b = b.status(status);
    if is_head {
        return b.body(Body::empty()).unwrap();
    }

    if let Some(max) = spec.max_open
        && s.in_flight.load(SeqCst) >= max
    {
        s.too_many.fetch_add(1, SeqCst);
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let bps = match spec.slow_from {
        Some((off, slow)) if start >= off => Some(slow),
        _ => spec.bytes_per_sec,
    };
    let drop_at = spec.drop_after.filter(|_| s.dropped_starts.lock().unwrap().insert(start));
    let stall_at = spec.stall_after.filter(|_| s.stalled_starts.lock().unwrap().insert(start));
    let st = Stream {
        data,
        pos: start,
        end,
        sent: 0,
        bps,
        drop_at,
        stall_at,
        t0: Instant::now(),
        bytes_sent: s.bytes_sent.clone(),
        _guard: InFlight::new(s.in_flight.clone(), &s.max_concurrent),
    };
    let stream = futures::stream::unfold(st, |mut st| async move {
        if st.pos >= st.end {
            return None;
        }
        let mut chunk = (16 * 1024).min(st.end - st.pos);
        if let Some(d) = st.drop_at {
            if st.sent >= d {
                st.pos = st.end;
                return Some((Err(io::Error::other("test server: dropped")), st));
            }
            chunk = chunk.min(d - st.sent);
        }
        if let Some(d) = st.stall_at {
            if st.sent >= d {
                futures::future::pending::<()>().await;
            }
            chunk = chunk.min(d - st.sent);
        }
        if let Some(bps) = st.bps {
            let due = st.t0 + Duration::from_secs_f64((st.sent + chunk) as f64 / bps as f64);
            tokio::time::sleep_until(due).await;
        }
        let bytes = st.data.slice(st.pos as usize..(st.pos + chunk) as usize);
        st.pos += chunk;
        st.sent += chunk;
        st.bytes_sent.fetch_add(chunk, SeqCst);
        Some((Ok::<Bytes, io::Error>(bytes), st))
    });
    b.body(Body::from_stream(stream)).unwrap()
}
