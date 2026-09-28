#![forbid(unsafe_code)]

//! Reusable local archive operations shared by frontends.

mod enumeration;
mod error;
mod inspect;
mod reference;
mod search;
mod sync;

pub use enumeration::{
    ThreadEnumerationReport, enumerate_repository_threads, enumerate_repository_threads_in_scope,
};
pub use error::EngineError;
pub use forgesync_store::{ArchiveStatus, ThreadDetail, ThreadPage};
pub use inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, archive_status, list_threads,
    show_thread,
};
pub use reference::{ReferenceParseError, RepositorySelector, ThreadSelector};
pub use search::{SearchMode, SearchRequest, search_threads};
pub use sync::{
    SyncProgress, SyncProgressStatus, SyncReport, SyncRequest, SyncThreadScope, sync_repositories,
};
