//! The local HTTP server on 127.0.0.1: four routes behind one layer of locks.

use super::guard::{bearer, check_host, check_origin};
use super::pairing::PairGate;
use crate::core::AppCore;
use crate::dto::CaptureRequest;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Extension, Json, Router};
use haul_engine::EngineError;
use serde::Deserialize;
use serde_json::json;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio_util::sync::CancellationToken;

pub const DEFAULT_PORTS: RangeInclusive<u16> = 17654..=17664;
const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_LINKS: usize = 5000;
const MAX_BROWSER_NAME: usize = 64;

/// The name an extension sends, safe to show and log: control characters removed, trimmed,
/// at most 64 characters, and "A browser" when nothing is left.
fn clean_browser_name(raw: &str) -> String {
    let kept: String = raw.chars().filter(|c| !c.is_control()).collect();
    let name: String = kept.trim().chars().take(MAX_BROWSER_NAME).collect();
    if name.is_empty() { "A browser".to_string() } else { name }
}

/// What the window hears about pairing requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairEvent {
    /// Request `n` from `browser` waits for Allow or Deny.
    Asked { n: u64, browser: String },
    /// Request `n` is over (answered, timed out, or the browser went away): close its question.
    Closed { n: u64 },
}

pub type OnPair = Arc<dyn Fn(PairEvent) + Send + Sync>;

/// Sends `Closed` however the request ends, including when the browser drops the connection.
struct ClosedOnDrop {
    n: u64,
    on_pair: OnPair,
}

impl Drop for ClosedOnDrop {
    fn drop(&mut self) {
        (self.on_pair)(PairEvent::Closed { n: self.n });
    }
}

struct Api {
    core: AppCore,
    gate: PairGate,
    on_pair: OnPair,
    port: u16,
    closing: Arc<AtomicBool>,
}

/// The running server. `stop` refuses new work and shuts it down.
pub struct ApiHandle {
    pub port: u16,
    stop: CancellationToken,
    closing: Arc<AtomicBool>,
    gate: PairGate,
}

impl ApiHandle {
    pub fn stop(&self) {
        self.closing.store(true, Ordering::SeqCst);
        // A pairing request waiting on the dialog would hold up the shutdown.
        self.gate.deny_waiting();
        self.stop.cancel();
    }
}

/// The caller's extension origin, once the locks have passed.
#[derive(Clone)]
struct CallerOrigin(String);

fn status_only(code: StatusCode, msg: &str) -> Response {
    (code, Json(json!({ "error": msg }))).into_response()
}

fn header_str(req: &Request, name: header::HeaderName) -> Option<&str> {
    req.headers().get(name).and_then(|v| v.to_str().ok())
}

/// Runs a store-touching `AppCore` call off the async threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, Panicked> {
    tokio::task::spawn_blocking(f).await.map_err(|_| Panicked)
}

/// A blocking call panicked.
struct Panicked;

impl IntoResponse for Panicked {
    fn into_response(self) -> Response {
        status_only(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
    }
}

/// The three locks, in order: Host, Origin, then the key (except on pair and status).
async fn locks(State(api): State<Arc<Api>>, mut req: Request, next: Next) -> Response {
    if api.closing.load(Ordering::SeqCst) {
        return status_only(StatusCode::SERVICE_UNAVAILABLE, "Haul is shutting down");
    }
    if !check_host(header_str(&req, header::HOST), api.port) {
        return status_only(StatusCode::MISDIRECTED_REQUEST, "wrong host");
    }
    let Some(origin) = check_origin(header_str(&req, header::ORIGIN)).map(str::to_string) else {
        return status_only(StatusCode::FORBIDDEN, "only the Haul extension may call this");
    };
    let open = matches!(req.uri().path(), "/v1/pair" | "/v1/status");
    if !open {
        let Some(token) = bearer(header_str(&req, header::AUTHORIZATION)).map(str::to_string) else {
            return status_only(StatusCode::UNAUTHORIZED, "not paired");
        };
        let core = api.core.clone();
        let o = origin.clone();
        match blocking(move || core.client_for(&token, &o)).await {
            Ok(Some(_)) => {}
            Ok(None) => return status_only(StatusCode::UNAUTHORIZED, "not paired"),
            Err(r) => return r.into_response(),
        }
    }
    req.extensions_mut().insert(CallerOrigin(origin));
    next.run(req).await
}

async fn status(
    State(api): State<Arc<Api>>,
    Extension(CallerOrigin(origin)): Extension<CallerOrigin>,
    headers: axum::http::HeaderMap,
) -> Response {
    let token = bearer(headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok())).map(str::to_string);
    let core = api.core.clone();
    let r = blocking(move || {
        let paired = token.is_some_and(|t| core.client_for(&t, &origin).is_some());
        (paired, core.active_count())
    })
    .await;
    match r {
        Ok((paired, active)) => Json(json!({
            "app": "haul",
            "version": env!("CARGO_PKG_VERSION"),
            "paired": paired,
            "active": active,
        }))
        .into_response(),
        Err(r) => r.into_response(),
    }
}

#[derive(Deserialize)]
struct PairBody {
    browser: String,
}

async fn pair(
    State(api): State<Arc<Api>>,
    Extension(CallerOrigin(origin)): Extension<CallerOrigin>,
    Json(body): Json<PairBody>,
) -> Response {
    let browser = clean_browser_name(&body.browser);
    let Ok(ticket) = api.gate.request() else {
        return status_only(StatusCode::TOO_MANY_REQUESTS, "a pairing request is already waiting");
    };
    tracing::info!(browser = %browser, "pairing requested");
    let n = ticket.number();
    let _closed = ClosedOnDrop { n, on_pair: api.on_pair.clone() };
    (api.on_pair)(PairEvent::Asked { n, browser: browser.clone() });
    if !ticket.wait().await {
        tracing::info!(browser = %browser, "pairing refused");
        return status_only(StatusCode::FORBIDDEN, "pairing was refused");
    }
    let core = api.core.clone();
    match blocking(move || core.pair_client(&browser, &origin)).await {
        Ok(Ok(token)) => Json(json!({ "token": token })).into_response(),
        Ok(Err(e)) => engine_error(e),
        Err(r) => r.into_response(),
    }
}

fn engine_error(e: EngineError) -> Response {
    let code = match &e {
        EngineError::Io(io) if io.kind() == std::io::ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    status_only(code, &e.to_string())
}

async fn capture(State(api): State<Arc<Api>>, Json(body): Json<CaptureRequest>) -> Response {
    let core = api.core.clone();
    match blocking(move || core.capture(body)).await {
        Ok(Ok(id)) => Json(json!({ "id": id })).into_response(),
        Ok(Err(e)) => engine_error(e),
        Err(r) => r.into_response(),
    }
}

#[derive(Deserialize)]
struct LinksBody {
    links: Vec<String>,
    page_title: Option<String>,
}

async fn links(State(api): State<Arc<Api>>, Json(body): Json<LinksBody>) -> Response {
    if body.links.len() > MAX_LINKS {
        return status_only(StatusCode::PAYLOAD_TOO_LARGE, "at most 5000 links at a time");
    }
    let core = api.core.clone();
    match blocking(move || core.add_browser_links(body.links, body.page_title.as_deref())).await {
        Ok(Ok(r)) => Json(json!({ "added": r.added, "skipped": r.skipped })).into_response(),
        Ok(Err(e)) => engine_error(e),
        Err(r) => r.into_response(),
    }
}

/// Binds the first free port in `ports` on 127.0.0.1, writes it to `port_file`, and serves until stopped.
pub async fn start_api(
    core: AppCore,
    gate: PairGate,
    on_pair: OnPair,
    ports: RangeInclusive<u16>,
    port_file: Option<PathBuf>,
) -> Result<ApiHandle, EngineError> {
    let mut listener = None;
    for p in ports.clone() {
        if let Ok(l) = tokio::net::TcpListener::bind(("127.0.0.1", p)).await {
            listener = Some(l);
            break;
        }
    }
    let Some(listener) = listener else {
        return Err(EngineError::Io(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            format!("no free port in {}-{}", ports.start(), ports.end()),
        )));
    };
    let port = listener.local_addr()?.port();
    if let Some(f) = port_file {
        std::fs::write(f, port.to_string())?;
    }
    let closing = Arc::new(AtomicBool::new(false));
    let api = Arc::new(Api { core, gate: gate.clone(), on_pair, port, closing: closing.clone() });
    let app = Router::new()
        // A POST, because Chrome sends no Origin header on an extension GET.
        .route("/v1/status", post(status))
        .route("/v1/pair", post(pair))
        .route("/v1/capture", post(capture))
        .route("/v1/links", post(links))
        .layer(middleware::from_fn_with_state(api.clone(), locks))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(api);
    let stop = CancellationToken::new();
    let s = stop.clone();
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).with_graceful_shutdown(s.cancelled_owned()).await {
            tracing::warn!(error = %e, "local API stopped");
        }
    });
    tracing::info!(port, "local API listening");
    Ok(ApiHandle { port, stop, closing, gate })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_name_strips_controls() {
        assert_eq!(clean_browser_name("Edge\u{1b}[31m\n"), "Edge[31m");
        assert_eq!(clean_browser_name("Microsoft\u{0}\u{7f} Edge"), "Microsoft Edge");
    }

    #[test]
    fn pairing_name_cut_to_64() {
        assert_eq!(clean_browser_name(&"é".repeat(100)).chars().count(), 64);
    }

    #[test]
    fn pairing_name_all_control_chars() {
        assert_eq!(clean_browser_name("\n\t\u{1b}"), "A browser");
        assert_eq!(clean_browser_name("   "), "A browser");
    }
}
