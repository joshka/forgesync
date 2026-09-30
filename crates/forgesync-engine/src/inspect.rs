//! # Offline archive inspection requests
//!
//! [`ThreadFilters`] expresses repository, kind, source-state, ordering, and pagination intent.
//! [`ThreadListRequest`] applies those filters to ordinary browsing. Search also uses the filters,
//! but chooses its own retrieval policy and default ordering. [`ThreadStateFilter`] and
//! [`ThreadSort`] are engine vocabulary rather than SQL or CLI argument representations.
//!
//! [`archive_status`] and [`list_repositories`] expose local store projections. [`list_threads`]
//! validates its page window, resolves display-name selectors to durable repository identities,
//! and asks the store for a page. [`show_thread`] resolves one repository and discussion number
//! before returning canonical content and selected child evidence. Missing local identities are
//! errors rather than triggers for provider acquisition.
//!
//! Callers supply an already opened archive and retain responsibility for closing it. These
//! operations do not create, migrate, refresh, discover credentials, or contact a provider. The
//! store owns SQL, tie ordering, payload decoding, and coverage projection; engine adapters
//! preserve typed failures while distinguishing a missing discussion from other store failures.
//!
//! Separate repository-resolution and projection reads are not one frozen database snapshot.
//! Retained detail can contain stale or incomplete evidence, and status counts summarize the
//! store's selected checks rather than proving freshness. Inspect coverage before relying on
//! completeness. Internal query adapters are shared with search without becoming public inspection
//! operations.

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
    /// Optional ordering policy; ordinary listing defaults to newest source update first.
    /// Search selects its own default when this is absent.
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

/// Returns local archive metadata and coverage counts without opening or changing the archive.
///
/// The report is a store projection, not a freshness check or an acquisition-completion proof.
pub async fn archive_status(archive: &Archive) -> Result<ArchiveStatus, EngineError> {
    archive.archive_status().await.map_err(Into::into)
}

/// Lists registered repositories without contacting GitHub or mutating the archive.
pub async fn list_repositories(archive: &Archive) -> Result<Vec<Repository>, EngineError> {
    archive.list_repositories().await.map_err(Into::into)
}

/// Lists retained discussions using validated pagination and registered repository scope.
///
/// The limit must be 1 through 1000 and the offset must fit SQLite's signed integer range.
/// Invalid pagination fails before repository lookup. Empty repository scope selects all
/// registered repositories; an unknown selector fails rather than producing an empty page.
/// Without an explicit sort, this operation uses newest source update order.
///
/// Repository resolution and the page read can observe separate database states. This operation
/// neither contacts GitHub nor changes the archive; coverage accompanies the retained results.
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

/// Shows retained canonical content and selected evidence for a local discussion selector.
///
/// Resolves the repository's current display name to its durable identity before looking up the
/// discussion number. Unknown repository and discussion targets have distinct engine errors.
/// The returned coverage must be inspected for stale or incomplete child evidence; this read
/// does not refresh the discussion or promise a single snapshot across its component queries.
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

/// Resolves current local display names to distinct durable repository identities.
///
/// Preserves first-selection order and removes duplicate identities. Empty input remains empty
/// for the store's all-repositories convention; any missing selector fails the whole resolution.
/// Restricted visibility keeps this shared query adapter out of the public inspection API.
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

/// Validates the 1–1000 limit and signed-SQLite offset without changing either value.
///
/// Returns a nonzero limit for query construction. Restricted visibility keeps this adapter shared
/// with search while leaving public callers at the request boundary.
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
