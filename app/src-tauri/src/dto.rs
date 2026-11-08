//! What the UI sees. Every field is camelCase in JSON.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadView {
    pub id: i64,
    pub package_id: Option<i64>,
    pub name: String,
    pub url: String,
    pub host: String,
    pub size: Option<u64>,
    pub written: u64,
    /// `DownloadState::as_str`.
    pub state: String,
    pub error: Option<String>,
    /// "high" | "normal" | "low".
    pub priority: String,
    pub position: i64,
    pub connections: u32,
    pub resumable: Option<bool>,
    pub target_dir: String,
    pub target_path: Option<String>,
    pub created_at: String,
    /// `[start, pos, end]` per saved range.
    pub segments: Vec<[u64; 3]>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageView {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedLimit {
    pub enabled: bool,
    pub bps: u64,
}

impl Default for SpeedLimit {
    fn default() -> Self {
        SpeedLimit { enabled: false, bps: 10_000_000 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub packages: Vec<PackageView>,
    pub downloads: Vec<DownloadView>,
    pub speed_limit: SpeedLimit,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressView {
    pub id: i64,
    pub written: u64,
    pub size: Option<u64>,
    pub speed_bps: u64,
    pub segments: Vec<[u64; 3]>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateView {
    pub id: i64,
    pub state: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StealView {
    pub id: i64,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub progress: Vec<ProgressView>,
    pub states: Vec<StateView>,
    pub steals: Vec<StealView>,
    pub resync: bool,
    /// The Collector changed: the UI reloads its snapshot.
    pub collector: bool,
    /// A post-processing run changed: the UI reloads the runs (and the snapshot, as files may have moved).
    pub postproc: bool,
}

pub use haul_engine::{CollectedLink, CollectorPackage, IntakeResult};

/// Which Collector links are hidden. `exts` are lowercase, without a leading dot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorFilters {
    pub html: bool,
    pub images: bool,
    pub small: bool,
    pub duplicates: bool,
    pub exts: Vec<String>,
}

impl Default for CollectorFilters {
    fn default() -> Self {
        CollectorFilters { html: true, images: true, small: false, duplicates: false, exts: Vec::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorSnapshot {
    pub packages: Vec<CollectorPackage>,
    pub links: Vec<CollectedLink>,
    pub filters: CollectorFilters,
    pub watch_clipboard: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResult {
    pub started: usize,
}

pub use haul_engine::postproc::{PostProcSettings, Run};
pub use haul_engine::rules::{Category, Destination, Rule};

/// Everything the Rules view shows. Saved passwords appear only as ids.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesConfig {
    pub settings: PostProcSettings,
    pub categories: Vec<Category>,
    pub rules: Vec<Rule>,
    pub passwords: Vec<i64>,
}

/// One download the browser extension caught (`POST /v1/capture`).
#[derive(Clone, Debug, Deserialize)]
pub struct CaptureRequest {
    pub url: String,
    pub filename: Option<String>,
    pub size: Option<u64>,
    pub referrer: Option<String>,
    pub user_agent: Option<String>,
    /// Joined `name=value; ...`, exactly as a `Cookie` header.
    pub cookies: Option<String>,
}

/// A browser extension paired with Haul, as the Browsers list shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedClientView {
    pub id: i64,
    pub browser: String,
    pub created_at: String,
    pub last_seen: Option<String>,
}
