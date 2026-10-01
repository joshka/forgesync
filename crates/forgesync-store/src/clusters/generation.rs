//! Committing one derived cluster generation.
//!
//! Proposals are validated before the write transaction, then resolved, matched to durable cluster
//! identities, and written under one fenced commit. Missing members and unseen clusters are retired
//! only when vector coverage is complete, so a partial generation cannot erase unseen evidence.
//! Local member decisions survive regeneration.

use std::collections::HashSet;
use std::fmt::Write as _;

use forgesync_core::document::DocumentRecipe;
use forgesync_core::timestamp::UtcTimestamp;
use sha2::{Digest, Sha256};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};

use crate::archive::Archive;
use crate::clusters::decisions::insert_cluster_event;
use crate::clusters::matching::{load_existing_clusters, match_cluster_identities};
use crate::clusters::{ClusterGenerationInput, ClusterGenerationResult};
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::sql::{push_bound_list, repository_row_id, thread_row_id, to_sql_integer};

/// Proposed cluster resolved to thread rows, with members sorted by row ID.
///
/// The sorted order defines the membership hash used as a new cluster's stable key.
#[derive(Debug)]
pub struct PreparedCluster {
    pub representative_id: i64,
    pub title: String,
    /// Thread rows and their direct representative scores.
    pub members: Vec<(i64, Option<f64>)>,
}

impl Archive {
    /// Saves a complete or partial generation under the active archive writer fence.
    ///
    /// Durable cluster IDs and local member decisions are preserved. Source discussions, documents,
    /// and vectors are not rewritten.
    pub async fn save_clusters_fenced(
        &self,
        token: &ArchiveLeaseToken,
        input: &ClusterGenerationInput,
        at: UtcTimestamp,
    ) -> Result<ClusterGenerationResult, StoreError> {
        validate_generation(input)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let repository_id = repository_row_id(&mut transaction, &input.repository).await?;
        let mut prepared = Vec::with_capacity(input.clusters.len());
        for cluster in &input.clusters {
            let representative_id =
                thread_row_id(&mut transaction, &cluster.representative).await?;
            let mut members = Vec::with_capacity(cluster.members.len());
            for member in &cluster.members {
                let thread_id = thread_row_id(&mut transaction, &member.thread).await?;
                members.push((thread_id, member.score_to_representative));
            }
            members.sort_by_key(|(thread_id, _)| *thread_id);
            prepared.push(PreparedCluster {
                representative_id,
                title: cluster.title.clone(),
                members,
            });
        }
        let existing = load_existing_clusters(&mut transaction, repository_id).await?;
        let matches = match_cluster_identities(&existing, &prepared);

        let cluster_count = to_sql_integer(prepared.len() as u64)?;
        let status = if input.complete_coverage {
            "complete"
        } else {
            "partial"
        };
        let run_id: i64 = sqlx::query_scalar(
            "INSERT INTO cluster_runs (repository_id, endpoint, model, recipe, recipe_version, status, eligible_threads, vector_threads, candidate_edges, cluster_count, member_count, started_at_us, finished_at_us) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?) RETURNING id",
        )
        .bind(repository_id)
        .bind(input.endpoint.trim())
        .bind(input.model.trim())
        .bind(input.recipe.as_str())
        .bind(i64::from(DocumentRecipe::VERSION))
        .bind(status)
        .bind(to_sql_integer(input.eligible_threads)?)
        .bind(to_sql_integer(input.vector_threads)?)
        .bind(to_sql_integer(input.candidate_edges)?)
        .bind(cluster_count)
        .bind(at.unix_microseconds())
        .bind(at.unix_microseconds())
        .fetch_one(&mut *transaction)
        .await?;

        let mut seen = Vec::with_capacity(prepared.len());
        let mut member_count = 0_u64;
        for (index, cluster) in prepared.iter().enumerate() {
            let cluster_id = upsert_generated_cluster(
                &mut transaction,
                repository_id,
                run_id,
                matches.get(&index).copied(),
                cluster,
                at,
            )
            .await?;
            seen.push(cluster_id);
            let member_ids = cluster
                .members
                .iter()
                .map(|(thread_id, _)| *thread_id)
                .collect::<Vec<_>>();
            remove_other_memberships(&mut transaction, cluster_id, &member_ids, at).await?;
            if input.complete_coverage {
                mark_missing_members_removed(&mut transaction, cluster_id, &member_ids, at).await?;
            }
            for (thread_id, score) in &cluster.members {
                upsert_generated_member(
                    &mut transaction,
                    cluster_id,
                    *thread_id,
                    *score,
                    run_id,
                    at,
                )
                .await?;
                member_count += 1;
            }
            insert_cluster_event(
                &mut transaction,
                cluster_id,
                Some(run_id),
                "generated",
                None,
                "",
                at,
            )
            .await?;
        }
        let retired_count = if input.complete_coverage {
            retire_unseen_clusters(&mut transaction, repository_id, &seen, at).await?
        } else {
            0
        };
        sqlx::query(
            "UPDATE cluster_runs SET status = ?, cluster_count = ?, member_count = ?, finished_at_us = ? WHERE id = ?",
        )
        .bind(status)
        .bind(cluster_count)
        .bind(to_sql_integer(member_count)?)
        .bind(at.unix_microseconds())
        .bind(run_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(ClusterGenerationResult {
            run_id: u64::try_from(run_id)
                .map_err(|_| StoreError::Corrupt("archive_count_invalid"))?,
            cluster_count: seen.len() as u64,
            member_count,
            retired_count,
            complete_coverage: input.complete_coverage,
        })
    }
}

/// Rejects duplicate threads, representatives outside their cluster, foreign-repository members,
/// out-of-range scores, and a false claim of complete vector coverage.
fn validate_generation(input: &ClusterGenerationInput) -> Result<(), StoreError> {
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

/// Updates a matched cluster in place, or inserts one keyed by its membership hash.
///
/// Reused IDs keep their stable key and local decisions; canonical choice and dismissal are not
/// touched.
async fn upsert_generated_cluster(
    connection: &mut SqliteConnection,
    repository_id: i64,
    run_id: i64,
    matched_id: Option<i64>,
    cluster: &PreparedCluster,
    at: UtcTimestamp,
) -> Result<i64, StoreError> {
    if let Some(id) = matched_id {
        sqlx::query(
            "UPDATE clusters SET status = 'active', representative_thread_id = ?, title = ?, last_run_id = ?, retired_at_us = NULL, updated_at_us = ? WHERE id = ? AND repository_id = ?",
        )
        .bind(cluster.representative_id)
        .bind(&cluster.title)
        .bind(run_id)
        .bind(at.unix_microseconds())
        .bind(id)
        .bind(repository_id)
        .execute(&mut *connection)
        .await?;
        return Ok(id);
    }
    Ok(sqlx::query_scalar(
        "INSERT INTO clusters (repository_id, stable_key, status, representative_thread_id, title, last_run_id, created_at_us, updated_at_us) VALUES (?, ?, 'active', ?, ?, ?, ?, ?) ON CONFLICT (repository_id, stable_key) DO UPDATE SET status = 'active', representative_thread_id = excluded.representative_thread_id, title = excluded.title, last_run_id = excluded.last_run_id, retired_at_us = NULL, updated_at_us = excluded.updated_at_us RETURNING id",
    )
    .bind(repository_id)
    .bind(cluster_stable_key(cluster))
    .bind(cluster.representative_id)
    .bind(&cluster.title)
    .bind(run_id)
    .bind(at.unix_microseconds())
    .bind(at.unix_microseconds())
    .fetch_one(&mut *connection)
    .await?)
}

/// Derives a deterministic, versioned key from sorted generated membership.
fn cluster_stable_key(cluster: &PreparedCluster) -> String {
    let mut hash = Sha256::new();
    hash.update(b"forgesync-cluster-members-v1\0");
    for (thread_id, _) in &cluster.members {
        hash.update(thread_id.to_be_bytes());
    }
    let mut key = String::from("members-");
    for byte in hash.finalize() {
        write!(&mut key, "{byte:02x}").expect("write to string");
    }
    key
}

/// Marks the members' active/excluded memberships in other clusters as removed.
async fn remove_other_memberships(
    connection: &mut SqliteConnection,
    cluster_id: i64,
    thread_ids: &[i64],
    at: UtcTimestamp,
) -> Result<(), StoreError> {
    for thread_id in thread_ids {
        sqlx::query(
            "UPDATE cluster_memberships SET state = 'removed', updated_at_us = ? WHERE thread_id = ? AND cluster_id <> ? AND state IN ('active', 'excluded')",
        )
        .bind(at.unix_microseconds())
        .bind(thread_id)
        .bind(cluster_id)
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
}

/// Marks members absent from a complete generation as removed; call only for complete coverage.
async fn mark_missing_members_removed(
    connection: &mut SqliteConnection,
    cluster_id: i64,
    thread_ids: &[i64],
    at: UtcTimestamp,
) -> Result<(), StoreError> {
    let mut statement = QueryBuilder::<Sqlite>::new(
        "UPDATE cluster_memberships SET state = 'removed', updated_at_us = ",
    );
    statement
        .push_bind(at.unix_microseconds())
        .push(" WHERE cluster_id = ")
        .push_bind(cluster_id);
    if !thread_ids.is_empty() {
        statement.push(" AND thread_id NOT IN (");
        push_bound_list(&mut statement, thread_ids);
        statement.push(")");
    }
    statement.push(" AND state IN ('active', 'excluded')");
    statement.build().execute(&mut *connection).await?;
    Ok(())
}

/// Adds or updates one generated member; a stored exclusion decision keeps it excluded.
async fn upsert_generated_member(
    connection: &mut SqliteConnection,
    cluster_id: i64,
    thread_id: i64,
    score: Option<f64>,
    run_id: i64,
    at: UtcTimestamp,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO cluster_memberships (cluster_id, thread_id, state, score_to_representative, first_seen_run_id, last_seen_run_id, created_at_us, updated_at_us) VALUES (?, ?, CASE WHEN EXISTS (SELECT 1 FROM cluster_member_decisions d WHERE d.cluster_id = ? AND d.thread_id = ? AND d.excluded = 1) THEN 'excluded' ELSE 'active' END, ?, ?, ?, ?, ?) ON CONFLICT (cluster_id, thread_id) DO UPDATE SET state = CASE WHEN EXISTS (SELECT 1 FROM cluster_member_decisions d WHERE d.cluster_id = excluded.cluster_id AND d.thread_id = excluded.thread_id AND d.excluded = 1) THEN 'excluded' ELSE 'active' END, score_to_representative = excluded.score_to_representative, last_seen_run_id = excluded.last_seen_run_id, updated_at_us = excluded.updated_at_us",
    )
    .bind(cluster_id)
    .bind(thread_id)
    .bind(cluster_id)
    .bind(thread_id)
    .bind(score)
    .bind(run_id)
    .bind(run_id)
    .bind(at.unix_microseconds())
    .bind(at.unix_microseconds())
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Retires active clusters absent from a complete generation, returning how many changed.
async fn retire_unseen_clusters(
    connection: &mut SqliteConnection,
    repository_id: i64,
    seen_cluster_ids: &[i64],
    at: UtcTimestamp,
) -> Result<u64, StoreError> {
    let mut statement =
        QueryBuilder::<Sqlite>::new("UPDATE clusters SET status = 'retired', retired_at_us = ");
    statement
        .push_bind(at.unix_microseconds())
        .push(", updated_at_us = ")
        .push_bind(at.unix_microseconds())
        .push(" WHERE repository_id = ")
        .push_bind(repository_id)
        .push(" AND status = 'active'");
    if !seen_cluster_ids.is_empty() {
        statement.push(" AND id NOT IN (");
        push_bound_list(&mut statement, seen_cluster_ids);
        statement.push(")");
    }
    Ok(statement
        .build()
        .execute(&mut *connection)
        .await?
        .rows_affected())
}
