#![forbid(unsafe_code)]

//! Reusable local archive operations shared by frontends.

mod error;
mod inspect;
mod reference;
mod search;

pub use error::EngineError;
pub use forgesync_store::{ArchiveStatus, ThreadDetail, ThreadPage};
pub use inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, archive_status, list_threads,
    show_thread,
};
pub use reference::{ReferenceParseError, RepositorySelector, ThreadSelector};
pub use search::{SearchMode, SearchRequest, search_threads};
