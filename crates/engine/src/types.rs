use serde::{Deserialize, Serialize};

pub type DownloadId = i64;
pub type PackageId = i64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DownloadState {
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    LinkExpired,
    Offline,
}

impl DownloadState {
    pub const ALL: [DownloadState; 7] = [
        DownloadState::Queued,
        DownloadState::Running,
        DownloadState::Paused,
        DownloadState::Completed,
        DownloadState::Failed,
        DownloadState::LinkExpired,
        DownloadState::Offline,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            DownloadState::Queued => "queued",
            DownloadState::Running => "running",
            DownloadState::Paused => "paused",
            DownloadState::Completed => "completed",
            DownloadState::Failed => "failed",
            DownloadState::LinkExpired => "link_expired",
            DownloadState::Offline => "offline",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// Queue priority. `Ord` sorts `High` first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Priority {
    High = 0,
    Normal = 1,
    Low = 2,
}

impl Priority {
    pub fn as_i64(self) -> i64 {
        self as i64
    }

    pub fn from_i64(v: i64) -> Priority {
        match v {
            0 => Priority::High,
            2 => Priority::Low,
            _ => Priority::Normal,
        }
    }
}

/// What a probe learned about a link.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeInfo {
    pub final_url: String,
    pub size: Option<u64>,
    pub resumable: bool,
    pub file_name: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_type: Option<String>,
}

/// Events the engine broadcasts to the UI.
#[derive(Clone, Debug, Serialize)]
pub enum Event {
    Progress {
        id: DownloadId,
        written: u64,
        size: Option<u64>,
        speed_bps: u64,
        /// (start, pos, end) per segment.
        segments: Vec<(u64, u64, u64)>,
    },
    StateChanged {
        id: DownloadId,
        state: DownloadState,
        error: Option<String>,
    },
    Steal {
        id: DownloadId,
        bytes: u64,
    },
    /// The row is gone from the store (sent after a remove, including a deferred one).
    Removed {
        id: DownloadId,
    },
    AllDone,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::EngineError;

    #[test]
    fn state_roundtrip() {
        for s in DownloadState::ALL {
            assert_eq!(DownloadState::parse(s.as_str()), Some(s));
        }
        assert_eq!(DownloadState::parse("nonsense"), None);
    }

    #[test]
    fn priority_orders_high_first() {
        let mut v = vec![Priority::Low, Priority::High, Priority::Normal];
        v.sort();
        assert_eq!(v, vec![Priority::High, Priority::Normal, Priority::Low]);
    }

    #[test]
    fn retryable_errors() {
        assert!(EngineError::Network("reset".into()).is_retryable());
        assert!(EngineError::Http(503).is_retryable());
        assert!(!EngineError::Http(404).is_retryable());
        assert!(!EngineError::Offline.is_retryable());
        assert!(!EngineError::LinkExpired(403).is_retryable());
        assert!(!EngineError::DiskFull.is_retryable());
    }
}
