use forgesync_store::StoreError;
use thiserror::Error;

use crate::reference::ReferenceParseError;

/// Errors returned by local Forgesync workflows.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EngineError {
    /// A command supplied a malformed repository or thread selector.
    #[error(transparent)]
    Reference(#[from] ReferenceParseError),
    /// A selected repository is not registered in the archive.
    #[error("repository is not present in the archive: {host}/{owner}/{name}")]
    RepositoryMissing {
        /// Canonical GitHub host.
        host: String,
        /// Current owner name.
        owner: String,
        /// Current repository name.
        name: String,
    },
    /// A selected thread is not present in the archive.
    #[error("thread is not present in the archive")]
    ThreadMissing,
    /// A search request has no searchable terms or an invalid advanced expression.
    #[error("search query is invalid")]
    InvalidSearchQuery,
    /// A page limit is zero or exceeds the local safety bound.
    #[error("page limit must be between 1 and 1000")]
    InvalidPageLimit,
    /// Pagination exceeds the range supported by SQLite.
    #[error("page offset is outside the supported range")]
    InvalidPageOffset,
    /// An archive operation failed.
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl EngineError {
    /// Returns the stable machine-readable classification for process output.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Reference(_) => "reference_invalid",
            Self::RepositoryMissing { .. } => "repository_missing",
            Self::ThreadMissing => "thread_missing",
            Self::InvalidSearchQuery => "search_query_invalid",
            Self::InvalidPageLimit => "page_limit_invalid",
            Self::InvalidPageOffset => "page_offset_invalid",
            Self::Store(StoreError::InvalidSearchQuery) => "search_query_invalid",
            Self::Store(error) => error.code(),
        }
    }
}
