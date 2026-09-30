//! # SQL mutations for generated clusters and memberships
//!
//! These operations borrow the generation transaction opened by `Archive::save_clusters_fenced`.
//! They create/finalize its run, reuse or insert cluster rows, move membership, preserve local
//! member decisions, and retire unseen groups only when the application authorizes complete
//! replacement. None of these helpers commits a transaction or performs analysis or provider I/O.
//!
//! `generation_apply` supplies one repository/run identity and timestamp throughout the writes.
//! Prepared member rows are sorted by `generation_input`; their order defines the versioned stable
//! key used when a proposal has no assigned durable identity. Existing identities instead retain
//! their key, allowing maintainer decisions to remain attached across membership changes.
//! SQL bind maps stay linear here so column/value correspondence can be reviewed directly.

use std::fmt::Write as _;

use forgesync_core::document::DocumentRecipe;
use forgesync_core::timestamp::UtcTimestamp;
use sha2::{Digest, Sha256};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};

use crate::clusters::ClusterGenerationInput;
use crate::clusters::generation_input::PreparedCluster;
use crate::error::StoreError;

/// Records the generation attempt before replacing current groups.
pub async fn insert_cluster_run(
    connection: &mut SqliteConnection,
    input: &ClusterGenerationInput,
    repository_id: i64,
    cluster_count: usize,
    at: UtcTimestamp,
) -> Result<i64, StoreError> {
    let vector_count =
        i64::try_from(input.vector_threads).map_err(|_| StoreError::IntegerOutOfRange)?;
    let eligible_count =
        i64::try_from(input.eligible_threads).map_err(|_| StoreError::IntegerOutOfRange)?;
    let edge_count =
        i64::try_from(input.candidate_edges).map_err(|_| StoreError::IntegerOutOfRange)?;
    let cluster_count = i64::try_from(cluster_count).map_err(|_| StoreError::IntegerOutOfRange)?;
    Ok(sqlx::query_scalar(
        "INSERT INTO cluster_runs (repository_id, endpoint, model, recipe, recipe_version, status, eligible_threads, vector_threads, candidate_edges, cluster_count, member_count, started_at_us, finished_at_us) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?) RETURNING id",
    )
    .bind(repository_id)
    .bind(input.endpoint.trim())
    .bind(input.model.trim())
    .bind(input.recipe.as_str())
    .bind(i64::from(DocumentRecipe::VERSION))
    .bind(if input.complete_coverage {
        "complete"
    } else {
        "partial"
    })
    .bind(eligible_count)
    .bind(vector_count)
    .bind(edge_count)
    .bind(cluster_count)
    .bind(at.unix_microseconds())
    .bind(at.unix_microseconds())
    .fetch_one(&mut *connection)
    .await?)
}

/// Preserves a stable cluster identity while updating generated evidence.
pub async fn upsert_generated_cluster(
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

    let stable_key = cluster_stable_key(cluster);
    Ok(sqlx::query_scalar(
        "INSERT INTO clusters (repository_id, stable_key, status, representative_thread_id, title, last_run_id, created_at_us, updated_at_us) VALUES (?, ?, 'active', ?, ?, ?, ?, ?) ON CONFLICT (repository_id, stable_key) DO UPDATE SET status = 'active', representative_thread_id = excluded.representative_thread_id, title = excluded.title, last_run_id = excluded.last_run_id, retired_at_us = NULL, updated_at_us = excluded.updated_at_us RETURNING id",
    )
    .bind(repository_id)
    .bind(stable_key)
    .bind(cluster.representative_id)
    .bind(&cluster.title)
    .bind(run_id)
    .bind(at.unix_microseconds())
    .bind(at.unix_microseconds())
    .fetch_one(&mut *connection)
    .await?)
}

/// Derives a deterministic key from current generated membership.
fn cluster_stable_key(cluster: &PreparedCluster) -> String {
    let mut hash = Sha256::new();
    hash.update(b"forgesync-cluster-members-v1\0");
    for (thread_id, _) in &cluster.members {
        hash.update(thread_id.to_be_bytes());
    }
    let digest = hash.finalize();
    let mut key = String::with_capacity(8 + digest.len() * 2);
    key.push_str("members-");
    for byte in digest {
        write!(&mut key, "{byte:02x}").expect("write to string");
    }
    key
}

/// Carries compatible local member decisions into a new generation.
pub async fn move_current_members(
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

/// Marks former members absent from a complete new generation.
pub async fn mark_missing_members_removed(
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
        for (index, thread_id) in thread_ids.iter().enumerate() {
            if index > 0 {
                statement.push(", ");
            }
            statement.push_bind(thread_id);
        }
        statement.push(")");
    }
    statement.push(" AND state IN ('active', 'excluded')");
    statement.build().execute(&mut *connection).await?;
    Ok(())
}

/// Adds or updates one generated member without losing local decision state.
pub async fn upsert_generated_member(
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

/// Retires groups absent from a complete replacement generation.
pub async fn retire_unseen_clusters(
    connection: &mut SqliteConnection,
    repository_id: i64,
    seen_cluster_ids: &[i64],
    at: UtcTimestamp,
) -> Result<usize, StoreError> {
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
        for (index, cluster_id) in seen_cluster_ids.iter().enumerate() {
            if index > 0 {
                statement.push(", ");
            }
            statement.push_bind(cluster_id);
        }
        statement.push(")");
    }
    let result = statement.build().execute(&mut *connection).await?;
    usize::try_from(result.rows_affected()).map_err(|_| StoreError::InvalidStoredCount)
}

/// Records the final outcome after generation writes finish.
pub async fn finish_cluster_run(
    connection: &mut SqliteConnection,
    run_id: i64,
    input: &ClusterGenerationInput,
    cluster_count: usize,
    member_count: u64,
    at: UtcTimestamp,
) -> Result<(), StoreError> {
    let cluster_count = i64::try_from(cluster_count).map_err(|_| StoreError::IntegerOutOfRange)?;
    let member_count = i64::try_from(member_count).map_err(|_| StoreError::IntegerOutOfRange)?;
    let expected_status = if input.complete_coverage {
        "complete"
    } else {
        "partial"
    };
    sqlx::query(
        "UPDATE cluster_runs SET status = ?, cluster_count = ?, member_count = ?, finished_at_us = ? WHERE id = ?",
    )
    .bind(expected_status)
    .bind(cluster_count)
    .bind(member_count)
    .bind(at.unix_microseconds())
    .bind(run_id)
    .execute(&mut *connection)
    .await?;
    Ok(())
}
