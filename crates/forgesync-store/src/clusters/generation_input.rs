//! # Validate and resolve proposed cluster membership
//!
//! Generation input is checked before opening the write transaction. Validation prevents duplicate
//! threads, invalid representatives, cross-repository membership, and false complete-coverage
//! claims. Once the archive opens its fenced transaction, `prepare_clusters` resolves domain
//! identities to stored discussion rows. It performs reads on that transaction and never commits
//! it.
//!
//! `PreparedCluster` holds SQL identities sorted by member row, shared by durable identity matching
//! and generation writes. This ordering also determines the membership hash used for new groups.
//! `thread_row_id` is reused by local decision writes, which require the same repository identity
//! checks. Candidate scoring remains in the engine; these operations validate persistence inputs.

use std::collections::HashSet;

use forgesync_core::identity::{RepositoryId, ThreadId};
use sqlx::SqliteConnection;

use crate::clusters::ClusterGenerationInput;
use crate::error::StoreError;

/// SQL-resolved generated membership, sorted by thread row identity before matching and hashing.
#[derive(Debug)]
pub struct PreparedCluster {
    /// Representative discussion row validated in the generation repository.
    pub representative_id: i64,
    /// Generated presentation title.
    pub title: String,
    /// Ordered discussion rows and their direct representative scores.
    pub members: Vec<(i64, Option<f64>)>,
}

/// Rejects a generation that cannot safely replace current membership, including duplicate
/// threads, representatives outside their cluster, and a false claim of complete vector coverage.
pub fn validate_generation(input: &ClusterGenerationInput) -> Result<(), StoreError> {
    if input.endpoint.trim().is_empty()
        || input.model.trim().is_empty()
        || (input.complete_coverage && input.eligible_threads != input.vector_threads)
        || input.vector_threads > input.eligible_threads
    {
        return Err(StoreError::InvalidClusterGeneration);
    }
    let mut seen = HashSet::new();
    for cluster in &input.clusters {
        if cluster.members.is_empty() || cluster.title.len() > 16_384 {
            return Err(StoreError::InvalidClusterGeneration);
        }
        let mut contains_representative = false;
        for member in &cluster.members {
            if member.thread.repository() != &input.repository
                || !seen.insert(member.thread.clone())
                || member
                    .score_to_representative
                    .is_some_and(|score| !score.is_finite() || !(-1.0..=1.0).contains(&score))
            {
                return Err(StoreError::InvalidClusterGeneration);
            }
            contains_representative |= member.thread == cluster.representative;
        }
        if !contains_representative || cluster.representative.repository() != &input.repository {
            return Err(StoreError::InvalidClusterGeneration);
        }
    }
    Ok(())
}

/// Resolves the repository whose cluster generation is being replaced.
pub async fn repository_row_id(
    connection: &mut SqliteConnection,
    repository: &RepositoryId,
) -> Result<i64, StoreError> {
    sqlx::query_scalar("SELECT id FROM repositories WHERE host = ? AND provider_id = ?")
        .bind(repository.host().as_str())
        .bind(repository.provider_id().as_str())
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(StoreError::RepositoryMissing)
}

/// Resolves candidate identities inside the active generation transaction and sorts member rows.
pub async fn prepare_clusters(
    connection: &mut SqliteConnection,
    input: &ClusterGenerationInput,
    repository_row_id: i64,
) -> Result<Vec<PreparedCluster>, StoreError> {
    let mut prepared = Vec::with_capacity(input.clusters.len());
    for cluster in &input.clusters {
        let representative_id =
            thread_row_id(connection, repository_row_id, &cluster.representative).await?;
        let mut members = Vec::with_capacity(cluster.members.len());
        for member in &cluster.members {
            let thread_id = thread_row_id(connection, repository_row_id, &member.thread).await?;
            members.push((thread_id, member.score_to_representative));
        }
        members.sort_by_key(|(thread_id, _)| *thread_id);
        prepared.push(PreparedCluster {
            representative_id,
            title: cluster.title.clone(),
            members,
        });
    }
    Ok(prepared)
}

/// Resolves a candidate discussion to its stored row identity.
pub async fn thread_row_id(
    connection: &mut SqliteConnection,
    repository_row_id: i64,
    thread: &ThreadId,
) -> Result<i64, StoreError> {
    let number = i64::try_from(thread.number().get()).map_err(|_| StoreError::IntegerOutOfRange)?;
    sqlx::query_scalar(
        "SELECT id FROM threads WHERE repository_id = ? AND provider_id = ? AND number = ?",
    )
    .bind(repository_row_id)
    .bind(thread.provider_id().as_str())
    .bind(number)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or(StoreError::ThreadMissing)
}
