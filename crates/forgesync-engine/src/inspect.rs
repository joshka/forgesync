//! Offline archive inspection requests.
//!
//! These operations never create, migrate, refresh, or contact a provider; a missing local identity
//! is an error, not a trigger for acquisition. Separate repository-resolution and projection reads
//! are not one frozen snapshot, and retained detail can contain stale or incomplete evidence.

use forgesync_core::content::{Repository, ThreadKind};
use forgesync_core::identity::ThreadReference;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::reads::{ArchiveStatus, ThreadDetail, ThreadPage, ThreadQuery};
pub use forgesync_store::reads::{ThreadSort, ThreadStateFilter};

use crate::error::EngineError;
use crate::query::{checked_page, repository_missing, resolve_repositories};
use crate::reference::{RepositorySelector, ThreadSelector};

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
        state: request.filters.state,
        match_expression: None,
        updated_since: None,
        sort: request.filters.sort.unwrap_or(ThreadSort::Updated),
        limit,
        offset,
    };
    archive.query_threads(&query).await.map_err(Into::into)
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
    thread_detail(
        archive,
        &ThreadReference::new(repository.id, selector.number()),
    )
    .await
}

/// Reads detail for an already resolved durable repository identity.
pub(crate) async fn thread_detail(
    archive: &Archive,
    reference: &ThreadReference,
) -> Result<ThreadDetail, EngineError> {
    archive
        .thread_detail(reference)
        .await
        .map_err(|error| match error {
            StoreError::ThreadMissing => EngineError::ThreadMissing,
            error => EngineError::Store(error),
        })
}
