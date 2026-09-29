//! Generation for durable clusters.

use std::fmt::Write as _;

use sha2::Digest;
use sqlx::Row;

use super::{
    Archive, ArchiveLeaseToken, ClusterGenerationInput, ClusterGenerationResult, DocumentRecipe,
    ExistingCluster, HashMap, HashSet, PreparedCluster, QueryBuilder, RepositoryId, Sha256, Sqlite,
    SqliteConnection, StoreError, ThreadId, UtcTimestamp, insert_cluster_event,
    require_active_archive_lease,
};

impl Archive {
    /// Saves a cluster generation under the active archive writer fence.
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
        let repository_row_id = repository_row_id(&mut transaction, &input.repository).await?;
        let prepared = prepare_clusters(&mut transaction, input, repository_row_id).await?;
        let existing = load_existing_clusters(&mut transaction, repository_row_id).await?;
        let matches = match_cluster_identities(&existing, &prepared);

        let run_id = insert_cluster_run(
            &mut transaction,
            input,
            repository_row_id,
            prepared.len(),
            at,
        )
        .await?;
        let mut seen_cluster_ids = Vec::with_capacity(prepared.len());
        let mut member_count = 0_u64;
        for (index, cluster) in prepared.iter().enumerate() {
            let matched_id = matches.get(&index).copied();
            let cluster_id = upsert_generated_cluster(
                &mut transaction,
                repository_row_id,
                run_id,
                matched_id,
                cluster,
                at,
            )
            .await?;
            seen_cluster_ids.push(cluster_id);
            let member_ids = cluster
                .members
                .iter()
                .map(|(thread_id, _)| *thread_id)
                .collect::<Vec<_>>();
            move_current_members(&mut transaction, cluster_id, &member_ids, at).await?;
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
                member_count = member_count.saturating_add(1);
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
            retire_unseen_clusters(&mut transaction, repository_row_id, &seen_cluster_ids, at)
                .await?
        } else {
            0
        };
        finish_cluster_run(
            &mut transaction,
            run_id,
            input,
            prepared.len(),
            member_count,
            at,
        )
        .await?;
        transaction.commit().await?;

        Ok(ClusterGenerationResult {
            run_id: u64::try_from(run_id).map_err(|_| StoreError::InvalidStoredCount)?,
            cluster_count: u64::try_from(prepared.len())
                .map_err(|_| StoreError::IntegerOutOfRange)?,
            member_count,
            retired_count: u64::try_from(retired_count)
                .map_err(|_| StoreError::IntegerOutOfRange)?,
            complete_coverage: input.complete_coverage,
        })
    }
}

/// Rejects a generation that cannot safely replace current membership, including duplicate
/// threads, representatives outside their cluster, and a false claim of complete vector coverage.
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

/// Resolves the repository whose cluster generation is being replaced.
async fn repository_row_id(
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

/// Validates generated groups before opening the write transaction.
async fn prepare_clusters(
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

/// Loads current stable keys so regenerated groups retain identity.
async fn load_existing_clusters(
    connection: &mut SqliteConnection,
    repository_id: i64,
) -> Result<Vec<ExistingCluster>, StoreError> {
    let rows = sqlx::query(
        "SELECT c.id, cm.thread_id FROM clusters c LEFT JOIN cluster_memberships cm ON cm.cluster_id = c.id AND cm.state IN ('active', 'excluded') WHERE c.repository_id = ? ORDER BY c.id, cm.thread_id",
    )
    .bind(repository_id)
    .fetch_all(&mut *connection)
    .await?;
    let mut clusters = Vec::<ExistingCluster>::new();
    for row in rows {
        let id: i64 = row.try_get("id")?;
        if clusters.last().is_none_or(|cluster| cluster.id != id) {
            clusters.push(ExistingCluster {
                id,
                members: HashSet::new(),
            });
        }
        let thread_id: Option<i64> = row.try_get("thread_id")?;
        if let Some(thread_id) = thread_id {
            clusters
                .last_mut()
                .expect("cluster row was inserted")
                .members
                .insert(thread_id);
        }
    }
    Ok(clusters)
}

/// Reuses durable cluster IDs by assigning the strongest membership overlaps first. The ordered
/// tie breaks keep equal evidence from producing different IDs on repeated builds.
fn match_cluster_identities(
    existing: &[ExistingCluster],
    generated: &[PreparedCluster],
) -> HashMap<usize, i64> {
    let mut candidates = Vec::<(usize, usize, usize, i64)>::new();
    for (generated_index, current) in generated.iter().enumerate() {
        let current_members = current
            .members
            .iter()
            .map(|(thread_id, _)| *thread_id)
            .collect::<HashSet<_>>();
        for previous in existing {
            let overlap = current_members.intersection(&previous.members).count();
            if overlap > 0 {
                let union = current_members.len() + previous.members.len() - overlap;
                candidates.push((overlap, union, generated_index, previous.id));
            }
        }
    }
    candidates.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| {
                (u128::try_from(right.0).unwrap_or(u128::MAX)
                    * u128::try_from(left.1).unwrap_or(u128::MAX))
                .cmp(
                    &(u128::try_from(left.0).unwrap_or(u128::MAX)
                        * u128::try_from(right.1).unwrap_or(u128::MAX)),
                )
            })
            .then_with(|| left.3.cmp(&right.3))
            .then_with(|| left.2.cmp(&right.2))
    });
    let mut used_generated = HashSet::new();
    let mut used_existing = HashSet::new();
    let mut matches = HashMap::new();
    for (_, _, generated_index, existing_id) in candidates {
        if !used_generated.contains(&generated_index) && !used_existing.contains(&existing_id) {
            used_generated.insert(generated_index);
            used_existing.insert(existing_id);
            matches.insert(generated_index, existing_id);
        }
    }
    matches
}

/// Records the generation attempt before replacing current groups.
async fn insert_cluster_run(
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
async fn move_current_members(
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

/// Retires groups absent from a complete replacement generation.
async fn retire_unseen_clusters(
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
async fn finish_cluster_run(
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
