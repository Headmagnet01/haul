//! Haul's download engine: probing, split-and-steal transfers, resume, queue and schedule.

pub mod error;
pub mod events;
pub mod naming;
pub mod probe;
pub mod segmenter;
pub mod store;
pub mod types;

pub use error::EngineError;
pub use events::EventBus;
pub use probe::{http_client, probe};
pub use store::{Download, NewDownload, Store};
pub use types::{DownloadId, DownloadState, Event, PackageId, Priority, ProbeInfo};
