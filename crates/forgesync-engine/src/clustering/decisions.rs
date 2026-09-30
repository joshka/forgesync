//! # Inspect clusters and record local maintainer choices
//!
//! [`show_cluster`] reads persisted detail without acquiring a writer lease. Dismiss and restore
//! operate on an archive-local cluster ID; exclude, include, and canonical selection first resolve
//! a discussion selector to its durable identity through local inspection. These operations do
//! not construct candidates, acquire provider evidence, or write back to GitHub.
//!
//! Each mutation obtains the current clock and an archive writer lease, then invokes a fenced
//! store decision operation. The store validates cluster/member relationships and owns the durable
//! transaction and event record. Member lookup happens before lease acquisition, so that lookup
//! alone does not prove membership at write time. Short decisions use a fixed lease without the
//! generation builder's heartbeat loop.
//!
//! Cleanup attempts lease release after the store operation, including failures. The operation
//! error takes precedence over a release error; a release failure after a successful write can
//! still return an error even though the decision is durable. Missing decision members are mapped
//! to the engine's invalid-decision error by the shared release adapter.
//!
//! Keeping maintainer actions separate from generation preserves authorship: derived analysis
//! proposes groups, while these functions explicitly record local triage choices. Callers retain
//! archive lifetime and presentation responsibility; this module installs no process diagnostics.

use forgesync_store::archive::Archive;
use forgesync_store::clusters::ClusterDetail;

use crate::clock::now_utc;
use crate::clustering::lease::{
    CLUSTER_LEASE_DURATION, finish_cluster_decision_lease, finish_cluster_lease,
};
use crate::error::EngineError;
use crate::reference::ThreadSelector;

/// Reads one persisted cluster and its current or excluded members.
pub async fn show_cluster(archive: &Archive, id: u64) -> Result<ClusterDetail, EngineError> {
    Ok(archive.cluster_detail(id).await?)
}

/// Records a local dismissal decision for one generated cluster.
pub async fn dismiss_cluster(archive: &Archive, id: u64, reason: &str) -> Result<(), EngineError> {
    let at = now_utc()?;
    let lease = archive
        .acquire_archive_lease(at, CLUSTER_LEASE_DURATION)
        .await?;
    let result = archive.dismiss_cluster_fenced(&lease, id, reason, at).await;
    finish_cluster_lease(archive, &lease, result).await
}

/// Clears a local dismissal decision for one generated cluster.
pub async fn restore_cluster(archive: &Archive, id: u64) -> Result<(), EngineError> {
    let at = now_utc()?;
    let lease = archive
        .acquire_archive_lease(at, CLUSTER_LEASE_DURATION)
        .await?;
    let result = archive.restore_cluster_fenced(&lease, id, at).await;
    finish_cluster_lease(archive, &lease, result).await
}

/// Excludes one current cluster member as a local maintainer decision.
pub async fn exclude_cluster_member(
    archive: &Archive,
    id: u64,
    reference: &ThreadSelector,
    reason: &str,
) -> Result<(), EngineError> {
    update_cluster_member(archive, id, reference, ClusterMemberAction::Exclude(reason)).await
}

/// Includes one previously excluded current cluster member.
pub async fn include_cluster_member(
    archive: &Archive,
    id: u64,
    reference: &ThreadSelector,
) -> Result<(), EngineError> {
    update_cluster_member(archive, id, reference, ClusterMemberAction::Include).await
}

/// Selects a current cluster member as the local canonical discussion.
pub async fn set_canonical_cluster_member(
    archive: &Archive,
    id: u64,
    reference: &ThreadSelector,
) -> Result<(), EngineError> {
    let thread = crate::inspect::show_thread(archive, reference)
        .await?
        .summary
        .discussion
        .id;
    let at = now_utc()?;
    let lease = archive
        .acquire_archive_lease(at, CLUSTER_LEASE_DURATION)
        .await?;
    let result = archive
        .set_cluster_canonical_fenced(&lease, id, &thread, at)
        .await;
    finish_cluster_decision_lease(archive, &lease, result).await
}

/// A member decision with rationale attached only to exclusion.
enum ClusterMemberAction<'a> {
    /// Exclude the selected member and retain the caller's reason.
    Exclude(&'a str),
    /// Clear the selected member's exclusion.
    Include,
}

/// Applies one local member decision under the archive's writer lease.
async fn update_cluster_member(
    archive: &Archive,
    id: u64,
    reference: &ThreadSelector,
    action: ClusterMemberAction<'_>,
) -> Result<(), EngineError> {
    let thread = crate::inspect::show_thread(archive, reference)
        .await?
        .summary
        .discussion
        .id;
    let at = now_utc()?;
    let lease = archive
        .acquire_archive_lease(at, CLUSTER_LEASE_DURATION)
        .await?;
    let result = match action {
        ClusterMemberAction::Exclude(reason) => {
            archive
                .exclude_cluster_member_fenced(&lease, id, &thread, reason, at)
                .await
        }
        ClusterMemberAction::Include => {
            archive
                .include_cluster_member_fenced(&lease, id, &thread, at)
                .await
        }
    };
    finish_cluster_decision_lease(archive, &lease, result).await
}
