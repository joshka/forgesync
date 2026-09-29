//! # Offline archive inspection requests
//!
//! Inspection types select thread state, sorting, and repository scope. The functions read archive
//! status, repository lists, thread pages, and thread detail through store projections.
//!
//! These operations stay local and read-only. The CLI and TUI can share them without sharing
//! argument parsing or terminal code; an inspect call does not initialize or refresh the archive.
//! Filters here express user intent while the store owns SQL implementation.

use std::num::NonZeroU32;

use forgesync_core::content::{Repository, ThreadKind};
use forgesync_core::identity::{RepositoryId, ThreadReference};
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::reads::{
    ArchiveStatus, ThreadDetail, ThreadPage, ThreadQuery, ThreadSort as StoreThreadSort,
    ThreadStateFilter as StoreThreadStateFilter,
};
use serde::Serialize;

use crate::error::EngineError;
use crate::reference::{RepositorySelector, ThreadSelector};

/// Source-state filter for a local discussion query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThreadStateFilter {
    /// Include open, closed, and unrecognized source states.
    #[default]
    All,
    /// Include only discussions whose source state is open.
    Open,
    /// Include only discussions whose source state is closed.
    Closed,
}

/// Sort order for a local discussion query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadSort {
    /// Rank full-text matches first; without a query, use update order.
    #[default]
    Relevance,
    /// Sort by source update time, newest first.
    Updated,
    /// Sort by source creation time, newest first.
    Created,
}

/// Repository, kind, state, sort, and pagination filters shared by local list operations.
#[derive(Clone, Debug)]
pub struct ThreadFilters {
    /// Repositories to include; empty selects every registered repository.
    pub repositories: Vec<RepositorySelector>,
    /// Optional issue or pull-request filter.
    pub kind: Option<ThreadKind>,
    /// Source open/closed state filter.
    pub state: ThreadStateFilter,
    /// Sort order; defaults to newest source update first.
    pub sort: Option<ThreadSort>,
    /// Maximum result count, from 1 through 1000.
    pub limit: u32,
    /// Number of matching rows to skip.
    pub offset: u64,
}

impl Default for ThreadFilters {
    /// Selects all repositories, kinds, and states at offset zero with a twenty-row bound.
    /// Sort selection remains with the inspection or search workflow using these filters.
    fn default() -> Self {
        Self {
            repositories: Vec::new(),
            kind: None,
            state: ThreadStateFilter::All,
            sort: None,
            limit: 20,
            offset: 0,
        }
    }
}

/// Request to list local threads with typed filters.
#[derive(Clone, Debug, Default)]
pub struct ThreadListRequest {
    /// Filters applied to the local thread collection.
    pub filters: ThreadFilters,
}

/// Returns read-only local archive metadata and coverage counts.
pub async fn archive_status(archive: &Archive) -> Result<ArchiveStatus, EngineError> {
    archive.archive_status().await.map_err(Into::into)
}

/// Lists registered repositories without contacting GitHub or mutating the archive.
pub async fn list_repositories(archive: &Archive) -> Result<Vec<Repository>, EngineError> {
    archive.list_repositories().await.map_err(Into::into)
}

/// Lists local discussions without contacting GitHub or mutating the archive.
pub async fn list_threads(
    archive: &Archive,
    request: &ThreadListRequest,
) -> Result<ThreadPage, EngineError> {
    let (limit, offset) = checked_page(request.filters.limit, request.filters.offset)?;
    let repositories = resolve_repositories(archive, &request.filters.repositories).await?;
    let query = ThreadQuery {
        repositories,
        kind: request.filters.kind,
        state: store_state_filter(request.filters.state),
        match_expression: None,
        updated_since: None,
        sort: store_sort(request.filters.sort.unwrap_or(ThreadSort::Updated)),
        limit,
        offset,
    };
    archive.query_threads(&query).await.map_err(Into::into)
}

/// Maps a read request state to the store query representation.
pub(crate) fn store_state_filter(state: ThreadStateFilter) -> StoreThreadStateFilter {
    match state {
        ThreadStateFilter::All => StoreThreadStateFilter::All,
        ThreadStateFilter::Open => StoreThreadStateFilter::Open,
        ThreadStateFilter::Closed => StoreThreadStateFilter::Closed,
    }
}

/// Maps presentation sort policy to the store's stable ordering.
pub(crate) fn store_sort(sort: ThreadSort) -> StoreThreadSort {
    match sort {
        ThreadSort::Relevance => StoreThreadSort::Relevance,
        ThreadSort::Updated => StoreThreadSort::Updated,
        ThreadSort::Created => StoreThreadSort::Created,
    }
}

/// Shows current local content and selected evidence for one explicit thread reference.
pub async fn show_thread(
    archive: &Archive,
    selector: &ThreadSelector,
) -> Result<ThreadDetail, EngineError> {
    let selected_repository = selector.repository();
    let repository = archive
        .find_repository(
            selected_repository.host(),
            selected_repository.owner(),
            selected_repository.name(),
        )
        .await?
        .ok_or_else(|| repository_missing(selected_repository))?;
    let reference = ThreadReference::new(repository.id, selector.number());
    archive
        .thread_detail(&reference)
        .await
        .map_err(|error| match error {
            StoreError::ThreadMissing => EngineError::ThreadMissing,
            error => EngineError::Store(error),
        })
}

/// Resolves selected repository names before constructing a local query.
pub(crate) async fn resolve_repositories(
    archive: &Archive,
    selectors: &[RepositorySelector],
) -> Result<Vec<RepositoryId>, EngineError> {
    let mut repositories = Vec::with_capacity(selectors.len());
    for selector in selectors {
        let repository = archive
            .find_repository(selector.host(), selector.owner(), selector.name())
            .await?
            .ok_or_else(|| repository_missing(selector))?;
        if !repositories.contains(&repository.id) {
            repositories.push(repository.id);
        }
    }
    Ok(repositories)
}

/// Validates a bounded page window before it reaches SQLite.
pub(crate) fn checked_page(limit: u32, offset: u64) -> Result<(NonZeroU32, u64), EngineError> {
    let limit = NonZeroU32::new(limit).filter(|value| value.get() <= 1000);
    let limit = limit.ok_or(EngineError::InvalidPageLimit)?;
    if offset > i64::MAX as u64 {
        return Err(EngineError::InvalidPageOffset);
    }
    Ok((limit, offset))
}

/// Builds the typed error for an absent repository selector.
fn repository_missing(selector: &RepositorySelector) -> EngineError {
    EngineError::RepositoryMissing {
        host: selector.host().as_str().to_owned(),
        owner: selector.owner().to_owned(),
        name: selector.name().to_owned(),
    }
}
