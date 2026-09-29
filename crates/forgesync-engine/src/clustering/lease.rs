//! # Bound cluster writes to one archive owner
//!
//! Lease helpers acquire or finish the archive coordination required around cluster builds and
//! decision changes. They ensure success and failure paths both release or record ownership
//! appropriately.
//!
//! The lease spans workflow coordination, while individual store writes use their own
//! transactions. These helpers keep cleanup near the operation that needs it without hiding the
//! analysis or decision steps.

use std::time::Duration;

use forgesync_store::archive::Archive;
use forgesync_store::leases::ArchiveLeaseToken;

use crate::documents::now_utc;
use crate::error::EngineError;

/// Writer fence lifetime, renewed every third of this interval during a generation build.
pub const CLUSTER_LEASE_DURATION: Duration = Duration::from_secs(180);

/// Releases the generation lease after success or failure.
pub async fn finish_cluster_lease<T>(
    archive: &Archive,
    lease: &ArchiveLeaseToken,
    operation: Result<T, forgesync_store::error::StoreError>,
) -> Result<T, EngineError> {
    finish_cluster_lease_result(archive, lease, operation.map_err(Into::into)).await
}

/// Releases the decision lease while preserving the original operation result.
pub async fn finish_cluster_decision_lease<T>(
    archive: &Archive,
    lease: &ArchiveLeaseToken,
    operation: Result<T, forgesync_store::error::StoreError>,
) -> Result<T, EngineError> {
    let operation = operation.map_err(|error| match error {
        forgesync_store::error::StoreError::ClusterMemberMissing => {
            EngineError::InvalidClusterDecision
        }
        error => error.into(),
    });
    finish_cluster_lease_result(archive, lease, operation).await
}

/// Combines operation and lease-release results without hiding the first failure.
pub async fn finish_cluster_lease_result<T>(
    archive: &Archive,
    lease: &ArchiveLeaseToken,
    operation: Result<T, EngineError>,
) -> Result<T, EngineError> {
    let release = match now_utc() {
        Ok(at) => archive
            .release_archive_lease(lease, at)
            .await
            .map_err(EngineError::from)
            .and_then(|released| {
                if released {
                    Ok(())
                } else {
                    Err(forgesync_store::error::StoreError::ArchiveLeaseLost.into())
                }
            }),
        Err(error) => Err(error),
    };
    match (operation, release) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
    }
}
