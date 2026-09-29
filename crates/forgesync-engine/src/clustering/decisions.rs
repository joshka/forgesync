//! Decisions cluster behavior.

use super::{
    Archive, CLUSTER_LEASE_DURATION, ClusterDetail, EngineError, ThreadSelector,
    finish_cluster_decision_lease, finish_cluster_lease, now_utc,
};

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

enum ClusterMemberAction<'a> {
    Exclude(&'a str),
    Include,
}

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
