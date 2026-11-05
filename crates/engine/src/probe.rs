//! Finding out what a link points to before downloading it.

use crate::error::EngineError;
use crate::naming::filename_from;
use crate::types::ProbeInfo;
use reqwest::header::{self, HeaderMap};
use reqwest::{Client, Response, StatusCode, redirect};
use std::time::Duration;

/// The engine's HTTP client: rustls (ring), at most 10 redirects,
/// 15 s connect timeout, 30 s read timeout.
pub fn http_client() -> Client {
    // Only the first call installs it; later calls return Err, which is fine.
    let _ = rustls::crypto::ring::default_provider().install_default();
    Client::builder()
        .redirect(redirect::Policy::limited(10))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .user_agent(concat!("Haul/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("building the HTTP client")
}

fn header_str(h: &HeaderMap, name: header::HeaderName) -> Option<String> {
    h.get(name).and_then(|v| v.to_str().ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn content_length(h: &HeaderMap) -> Option<u64> {
    header_str(h, header::CONTENT_LENGTH).and_then(|v| v.parse().ok())
}

/// Total size from `Content-Range: bytes a-b/total` (`*` means unknown).
pub fn content_range_total(h: &HeaderMap) -> Option<u64> {
    let v = header_str(h, header::CONTENT_RANGE)?;
    v.rsplit_once('/')?.1.trim().parse().ok()
}

fn info_from(resp: &Response, size: Option<u64>, resumable: bool) -> ProbeInfo {
    let h = resp.headers();
    let cd = header_str(h, header::CONTENT_DISPOSITION);
    ProbeInfo {
        final_url: resp.url().to_string(),
        size,
        resumable,
        file_name: filename_from(cd.as_deref(), resp.url()),
        etag: header_str(h, header::ETAG),
        last_modified: header_str(h, header::LAST_MODIFIED),
        content_type: header_str(h, header::CONTENT_TYPE),
    }
}

/// Probes a link: `HEAD` first, then `GET` with `Range: bytes=0-0` if `HEAD` failed,
/// gave no length, or didn't promise ranges.
pub async fn probe(client: &Client, url: &str, headers: &HeaderMap) -> Result<ProbeInfo, EngineError> {
    url::Url::parse(url).map_err(|e| EngineError::Network(format!("bad url: {e}")))?;

    if let Ok(resp) = client.head(url).headers(headers.clone()).send().await
        && resp.status().is_success()
    {
        let h = resp.headers();
        let len = content_length(h).filter(|&l| l > 0);
        let ranges = header_str(h, header::ACCEPT_RANGES).is_some_and(|v| v.to_ascii_lowercase().contains("bytes"));
        if let (Some(len), true) = (len, ranges) {
            return Ok(info_from(&resp, Some(len), true));
        }
    }

    let resp = client.get(url).headers(headers.clone()).header(header::RANGE, "bytes=0-0").send().await?;
    let info = match resp.status() {
        StatusCode::PARTIAL_CONTENT => {
            let total = content_range_total(resp.headers());
            info_from(&resp, total, total.is_some())
        }
        StatusCode::RANGE_NOT_SATISFIABLE => {
            // Only an empty file can't satisfy bytes=0-0.
            info_from(&resp, Some(0), false)
        }
        s if s.is_success() => info_from(&resp, content_length(resp.headers()), false),
        s => return Err(EngineError::from_status(s.as_u16())),
    };
    drop(resp); // don't read the body
    Ok(info)
}
