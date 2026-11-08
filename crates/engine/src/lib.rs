//! Haul's download engine: probing, split-and-steal transfers, resume, queue and schedule.

pub mod error;
pub mod events;
pub mod grabber;
pub mod links;
pub mod naming;
pub mod postproc;
pub mod probe;
pub mod queue;
pub mod ratelimit;
pub mod rules;
pub mod schedule;
pub mod secret;
pub mod segmenter;
pub mod store;
pub mod transfer;
pub mod types;

pub use error::EngineError;
pub use events::EventBus;
pub use grabber::{
    ArchiveKind, CollectedLink, CollectorPackage, CollectorState, DupKind, Grabber, IntakeResult, ProbeFn,
    StartDefaults, http_prober,
};
pub use links::extract_links;
pub use probe::{http_client, probe};
pub use queue::Queue;
pub use ratelimit::{HostLimiter, RateLimiter};
pub use schedule::{Schedule, WhenDone, is_within_window};
pub use segmenter::{MIB, Range};
pub use store::{Batch, CollectorLink, Download, NewDownload, Package, PairedClient, Store};
pub use transfer::{TransferConfig, TransferCtx, TransferJob, TransferOutcome, run_transfer};
pub use types::{DownloadId, DownloadState, Event, LinkStatus, PackageId, Priority, ProbeInfo};
