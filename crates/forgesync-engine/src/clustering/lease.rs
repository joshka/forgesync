//! # Bound cluster writes to one archive owner
//!
//! `ClusterBuildLease` owns the fence and child cancellation scope during generation construction.
//! The build borrows both capabilities while loading vectors and persisting a generation. Decision
//! changes use the smaller release helpers below because they do not run the long analysis phase.
//!
//! The lease spans workflow coordination, while individual store writes use their own
//! transactions. These helpers keep cleanup near the operation that needs it without hiding the
//! analysis or decision steps.
//!
//! Build completion polls the operation alongside renewal and caller cancellation. An interruption
//! cancels the child scope and awaits the operation rather than dropping it: blocking graph work
//! and durable cleanup must finish before fence release. Local renewal failure never cancels the
//! caller token. The triggering operation error takes precedence if release also fails.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use forgesync_store::archive::Archive;
use forgesync_store::leases::ArchiveLeaseToken;
use tokio::time::{Instant, interval_at};
use tokio_util::sync::CancellationToken;

use crate::clock::now_utc;
use crate::clustering::ClusterBuildReport;
use crate::error::EngineError;

/// Writer fence lifetime, renewed every third of this interval during a generation build.
pub const CLUSTER_LEASE_DURATION: Duration = Duration::from_secs(180);

/// Active cluster-build fence and cooperative cancellation scope.
/// The build borrows these capabilities; completion waits for its cleanup before fence release.
pub struct ClusterBuildLease<'a> {
    /// Archive holding the lease and receiving heartbeat renewals.
    archive: &'a Archive,
    /// Fence shared by vector reads and generation persistence.
    pub token: ArchiveLeaseToken,
    /// Child token cancelled on caller interruption or renewal failure.
    pub cancellation: CancellationToken,
}

impl<'a> ClusterBuildLease<'a> {
    /// Acquires the fence before vector loading and creates a local cancellation scope.
    pub async fn acquire(
        archive: &'a Archive,
        cancellation: &CancellationToken,
    ) -> Result<Self, EngineError> {
        let started_at = now_utc()?;
        let token = archive
            .acquire_archive_lease(started_at, CLUSTER_LEASE_DURATION)
            .await?;
        Ok(Self {
            archive,
            token,
            cancellation: cancellation.child_token(),
        })
    }

    /// Renews while the build runs, waits for cooperative cleanup, then releases the fence.
    /// A build or interruption error takes precedence over a later release error.
    pub async fn complete(
        &self,
        operation: impl Future<Output = Result<ClusterBuildReport, EngineError>>,
        cancellation: &CancellationToken,
    ) -> Result<ClusterBuildReport, EngineError> {
        let result = self.wait(operation, cancellation).await;
        finish_cluster_lease_result(self.archive, &self.token, result).await
    }

    /// Polls the build alongside caller cancellation and periodic lease renewal.
    async fn wait(
        &self,
        operation: impl Future<Output = Result<ClusterBuildReport, EngineError>>,
        cancellation: &CancellationToken,
    ) -> Result<ClusterBuildReport, EngineError> {
        let mut operation = std::pin::pin!(operation);
        let period = CLUSTER_LEASE_DURATION / 3;
        let mut heartbeat = interval_at(Instant::now() + period, period);
        loop {
            tokio::select! {
                result = &mut operation => return result,
                _ = cancellation.cancelled() => {
                    return self.interrupt(operation.as_mut(), EngineError::ClusteringCancelled).await;
                }
                _ = heartbeat.tick() => {
                    if let Err(error) = self.renew().await {
                        return self.interrupt(operation.as_mut(), error).await;
                    }
                }
            }
        }
    }

    /// Signals the child scope and drains the build before returning the triggering failure.
    async fn interrupt(
        &self,
        operation: Pin<&mut impl Future<Output = Result<ClusterBuildReport, EngineError>>>,
        error: EngineError,
    ) -> Result<ClusterBuildReport, EngineError> {
        self.cancellation.cancel();
        let _ = operation.await;
        Err(error)
    }

    /// Advances the fence expiry using the cluster-build heartbeat policy.
    async fn renew(&self) -> Result<(), EngineError> {
        let now = now_utc()?;
        self.archive
            .heartbeat_archive_lease(&self.token, now, CLUSTER_LEASE_DURATION)
            .await?;
        Ok(())
    }
}

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

#[cfg(test)]
#[path = "lease_tests.rs"]
mod tests;
