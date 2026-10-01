//! Repository resolution and page validation shared by inspection, search, and cluster browsing.

use std::num::NonZeroU32;

use forgesync_core::identity::RepositoryId;
use forgesync_store::archive::Archive;

use crate::error::EngineError;
use crate::reference::RepositorySelector;

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
