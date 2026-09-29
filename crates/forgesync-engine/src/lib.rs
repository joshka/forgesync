#![forbid(unsafe_code)]

//! # Local-first application workflows
//!
//! The engine coordinates provider acquisition, archive writes, offline inspection, search,
//! embeddings, and cluster analysis. It accepts an already opened `Archive`; the caller chooses
//! whether that handle is writable and supplies cancellation for long-running work. The engine
//! never owns CLI argument parsing or terminal rendering.
//!
//! `sync` and `enumeration` acquire source evidence through the GitHub adapter. `refresh` composes
//! acquisition with optional derived analysis. `documents`, `embeddings`, and `clustering` build
//! local search and triage material from archived discussions. `inspect`, `search`, and `runs`
//! expose offline reads and retry workflows. Provider DTOs are normalized before they reach the
//! store; SQL row details do not escape the store.
//!
//! Workflows return typed reports that preserve partial success. A failed discussion or resource
//! family should be visible without discarding successful work from the same run.

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
