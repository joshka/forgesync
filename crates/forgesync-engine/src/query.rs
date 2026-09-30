//! # Shared local-query adaptation
//!
//! Inspection, search, and cluster browsing share repository resolution, bounded pagination,
//! and conversion from engine filter vocabulary to store query vocabulary. These helpers perform
//! that boundary adaptation without defining a second public inspection API.
//!
//! Repository resolution reads registered display names and returns durable identities in first
//! selection order. Pagination checks protect SQLite's integer range and the engine's page bound.
//! Pure enum mappings remain exhaustive so their complete vocabulary is visible in one place.
//!
//! The module is private to the engine; its functions are public within that boundary for sibling
//! workflows. Callers retain archive lifetime, operation defaults, snapshot consistency, and
//! projection policy. Resolution does not acquire provider data or guarantee a frozen query scope.

use std::num::NonZeroU32;

use forgesync_core::identity::RepositoryId;
use forgesync_store::archive::Archive;
use forgesync_store::reads::{
    ThreadSort as StoreThreadSort, ThreadStateFilter as StoreThreadStateFilter,
};

use crate::error::EngineError;
use crate::inspect::{ThreadSort, ThreadStateFilter};
use crate::reference::RepositorySelector;

/// Maps a read request state to the store query representation.
pub fn store_state_filter(state: ThreadStateFilter) -> StoreThreadStateFilter {
    match state {
        ThreadStateFilter::All => StoreThreadStateFilter::All,
        ThreadStateFilter::Open => StoreThreadStateFilter::Open,
        ThreadStateFilter::Closed => StoreThreadStateFilter::Closed,
    }
}

/// Maps presentation sort policy to the store's stable ordering.
pub fn store_sort(sort: ThreadSort) -> StoreThreadSort {
    match sort {
        ThreadSort::Relevance => StoreThreadSort::Relevance,
        ThreadSort::Updated => StoreThreadSort::Updated,
        ThreadSort::Created => StoreThreadSort::Created,
    }
}

/// Resolves current local display names to distinct durable repository identities.
///
/// Preserves first-selection order and removes duplicate identities. Empty input remains empty
/// for the store's all-repositories convention; any missing selector fails the whole resolution.
/// The private module keeps this shared query adapter out of the public inspection API.
pub async fn resolve_repositories(
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
/// Returns a nonzero limit for query construction. The private module keeps this adapter shared
/// with search while leaving public callers at the request boundary.
pub fn checked_page(limit: u32, offset: u64) -> Result<(NonZeroU32, u64), EngineError> {
    let limit = NonZeroU32::new(limit).filter(|value| value.get() <= 1000);
    let limit = limit.ok_or(EngineError::InvalidPageLimit)?;
    if offset > i64::MAX as u64 {
        return Err(EngineError::InvalidPageOffset);
    }
    Ok((limit, offset))
}

/// Builds the typed error for an absent repository selector.
pub fn repository_missing(selector: &RepositorySelector) -> EngineError {
    EngineError::RepositoryMissing {
        host: selector.host().as_str().to_owned(),
        owner: selector.owner().to_owned(),
        name: selector.name().to_owned(),
    }
}
