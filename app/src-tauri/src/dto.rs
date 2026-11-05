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
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddResult {
    pub added: usize,
    pub package_id: Option<i64>,
}
