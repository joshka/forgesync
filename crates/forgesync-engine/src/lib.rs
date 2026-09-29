#![forbid(unsafe_code)]

//! Workflows over an already opened local archive.
//!
//! [`sync`] acquires provider resources and applies observations; [`refresh`] coordinates sync and
//! selected analysis. [`inspect`] and [`search`] answer local reads. [`clustering`] creates and
//! manages duplicate groups, while [`runs`] plans retry work. Frontends provide the archive,
//! clients, requests, and cancellation; this crate does not resolve process configuration.

pub mod clustering;
pub mod documents;
pub mod embedding_client;
pub mod embeddings;
pub mod enumeration;
pub mod error;
pub mod exact_search;
pub mod inspect;
pub mod reference;
pub mod refresh;
pub mod runs;
pub mod search;
pub mod sync;

pub use forgesync_store::clusters::{
    ClusterDetail, ClusterLifecycle, ClusterMember, ClusterMemberRole, ClusterMemberState,
    ClusterPage, ClusterSummary,
};
pub use forgesync_store::reads::{
    ArchiveStatus, ThreadDetail, ThreadPage, ThreadSummary, ThreadTimelineEvent,
};
pub use forgesync_store::runs::{RunStatus, SyncJobStatus};
