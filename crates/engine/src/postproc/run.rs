//! A post-processing run as the UI sees it, and what it keeps between steps.

use crate::types::{DownloadId, PackageId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Verify,
    Extract,
    Sort,
    Scan,
    Notify,
}

impl StepKind {
    pub const ALL: [StepKind; 5] =
        [StepKind::Verify, StepKind::Extract, StepKind::Sort, StepKind::Scan, StepKind::Notify];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Pending,
    Running,
    Done,
    Skipped,
    Failed,
    NeedsPassword,
}

impl StepState {
    pub fn finished(self) -> bool {
        matches!(self, StepState::Done | StepState::Skipped)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// Auto mode is off: waiting for the user to press Process.
    Held,
    Queued,
    Running,
    /// A file is downloading again after a checksum mismatch.
    Waiting,
    NeedsPassword,
    Failed,
    Done,
}

impl RunState {
    pub const ALL: [RunState; 7] = [
        RunState::Held,
        RunState::Queued,
        RunState::Running,
        RunState::Waiting,
        RunState::NeedsPassword,
        RunState::Failed,
        RunState::Done,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RunState::Held => "held",
            RunState::Queued => "queued",
            RunState::Running => "running",
            RunState::Waiting => "waiting",
            RunState::NeedsPassword => "needs_password",
            RunState::Failed => "failed",
            RunState::Done => "done",
        }
    }

    pub fn parse(s: &str) -> Option<RunState> {
        RunState::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub kind: StepKind,
    pub state: StepState,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: i64,
    pub package_id: Option<PackageId>,
    pub download_id: Option<DownloadId>,
    pub state: RunState,
    pub steps: Vec<Step>,
    /// UTC, `YYYY-MM-DDTHH:MM:SSZ`.
    pub updated_at: String,
}

/// `steps_json` in the store: the steps plus what later steps and restarts need.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunData {
    pub steps: Vec<Step>,
    /// Downloads whose checksum matched.
    pub verified: Vec<DownloadId>,
    /// The extraction folder, once reserved.
    pub extracted_to: Option<String>,
    /// Where sorting put each item.
    pub moved: Vec<String>,
}

impl RunData {
    pub fn fresh() -> RunData {
        RunData {
            steps: StepKind::ALL.iter().map(|&kind| Step { kind, state: StepState::Pending, detail: None }).collect(),
            ..Default::default()
        }
    }

    pub fn step_mut(&mut self, k: StepKind) -> &mut Step {
        if !self.steps.iter().any(|s| s.kind == k) {
            self.steps.push(Step { kind: k, state: StepState::Pending, detail: None });
        }
        self.steps.iter_mut().find(|s| s.kind == k).unwrap()
    }

    pub fn step(&self, k: StepKind) -> Option<&Step> {
        self.steps.iter().find(|s| s.kind == k)
    }
}
