//! Inspect clusters and record local maintainer decisions.
//!
//! Member selectors are resolved before the writer lease is acquired, so that lookup alone does not
//! prove membership at write time; the store validates it inside its fenced transaction.

use forgesync_core::identity::ThreadId;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use forgesync_store::clusters::ClusterDetail;
use forgesync_store::error::StoreError;
use forgesync_store::leases::ArchiveLeaseToken;
use tokio_util::sync::CancellationToken;

use crate::clock::now_utc;
use crate::clustering::build::CLUSTER_LEASE_DURATION;
use crate::error::EngineError;
use crate::inspect::show_thread;
use crate::lease::with_writer_lease;
use crate::reference::ThreadSelector;

/// Reads one persisted cluster and its current or excluded members.
pub async fn show_cluster(archive: &Archive, id: u64) -> Result<ClusterDetail, EngineError> {
    Ok(archive.cluster_detail(id).await?)
}

/// Records a local dismissal decision for one generated cluster.
pub async fn dismiss_cluster(archive: &Archive, id: u64, reason: &str) -> Result<(), EngineError> {
    decide(archive, async |lease, at| {
        archive.dismiss_cluster_fenced(lease, id, reason, at).await
    })
    .await
}

/// Clears a local dismissal decision for one generated cluster.
pub async fn restore_cluster(archive: &Archive, id: u64) -> Result<(), EngineError> {
    decide(archive, async |lease, at| {
        archive.restore_cluster_fenced(lease, id, at).await
    })
    .await
}

/// Excludes one current cluster member as a local maintainer decision.
pub async fn exclude_cluster_member(
    archive: &Archive,
    id: u64,
    reference: &ThreadSelector,
    reason: &str,
) -> Result<(), EngineError> {
    let thread = selected_thread(archive, reference).await?;
    decide(archive, async |lease, at| {
        archive
            .exclude_cluster_member_fenced(lease, id, &thread, reason, at)
            .await
    })
    .await
}

/// Includes one previously excluded current cluster member.
pub async fn include_cluster_member(
    archive: &Archive,
    id: u64,
    reference: &ThreadSelector,
) -> Result<(), EngineError> {
    let thread = selected_thread(archive, reference).await?;
    decide(archive, async |lease, at| {
        archive
            .include_cluster_member_fenced(lease, id, &thread, at)
            .await
    })
    .await
}

/// Selects a current cluster member as the local canonical discussion.
pub async fn set_canonical_cluster_member(
    archive: &Archive,
    id: u64,
    reference: &ThreadSelector,
) -> Result<(), EngineError> {
    let thread = selected_thread(archive, reference).await?;
    decide(archive, async |lease, at| {
        archive
            .set_cluster_canonical_fenced(lease, id, &thread, at)
            .await
    })
    .await
}

/// Resolves a member selector to its durable thread identity.
async fn selected_thread(
    archive: &Archive,
    reference: &ThreadSelector,
) -> Result<ThreadId, EngineError> {
    Ok(show_thread(archive, reference).await?.summary.discussion.id)
}

/// Applies one fenced store decision; a missing member target becomes an invalid decision.
async fn decide(
    archive: &Archive,
    write: impl AsyncFnOnce(&ArchiveLeaseToken, UtcTimestamp) -> Result<(), StoreError>,
) -> Result<(), EngineError> {
    let at = now_utc()?;
    with_writer_lease(
        archive,
        CLUSTER_LEASE_DURATION,
        &CancellationToken::new(),
        async |lease, _| {
            write(lease, at).await.map_err(|error| match error {
                StoreError::ClusterMemberMissing => EngineError::InvalidClusterDecision,
                error => error.into(),
            })
        },
    )
    .await
}
