use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::num::NonZeroU32;

use forgesync_core::{
    DocumentRecipe, Repository, RepositoryId, ThreadId, ThreadNumber, ThreadReference, UtcTimestamp,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{QueryBuilder, Row, Sqlite, SqliteConnection};

use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::reads::{coverage_for_kind, load_thread_coverage};
use crate::{Archive, StoreError, ThreadSummary};

/// One generated member and its default score to the graph representative.
#[derive(Clone, Debug)]
pub struct ClusterMemberInput {
    /// Stable source discussion identity.
    pub thread: ThreadId,
    /// Cosine or deterministic reference score to the generated representative.
    pub score_to_representative: Option<f64>,
}

/// One connected component produced by the deterministic clustering engine.
#[derive(Clone, Debug)]
pub struct ClusterInput {
    /// Graph-selected representative before local canonical decisions are applied.
    pub representative: ThreadId,
    /// Display title derived from the representative's current source title.
    pub title: String,
    /// Current generated members in stable identity order.
    pub members: Vec<ClusterMemberInput>,
}

/// Complete or partial cluster generation input for one repository and vector service.
#[derive(Clone, Debug)]
pub struct ClusterGenerationInput {
    /// Repository whose current open discussions were clustered.
    pub repository: RepositoryId,
    /// Compatible embedding endpoint identity, without credentials.
    pub endpoint: String,
    /// Compatible embedding model identity.
    pub model: String,
    /// Document recipe used for the vectors.
    pub recipe: DocumentRecipe,
    /// True only when every eligible current discussion has a compatible vector.
    pub complete_coverage: bool,
    /// Number of eligible discussions in the requested state scope.
    pub eligible_threads: u64,
    /// Number of eligible discussions with compatible current vectors.
    pub vector_threads: u64,
    /// Number of similarity and reference edges retained after fanout pruning.
    pub candidate_edges: u64,
    /// Generated clusters, including single-member orphan clusters when selected.
    pub clusters: Vec<ClusterInput>,
}

/// Durable counts and run identity produced by saving one cluster generation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClusterGenerationResult {
    /// Archive-local cluster run ID.
    pub run_id: u64,
    /// Number of generated clusters in this run.
    pub cluster_count: u64,
    /// Number of generated memberships in this run.
    pub member_count: u64,
    /// Number of previous generated clusters retired by a complete run.
    pub retired_count: u64,
    /// Whether this run had complete vector coverage.
    pub complete_coverage: bool,
}

/// Whether a generated cluster is current or retained as historical context.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterLifecycle {
    /// The cluster belongs to the latest complete or partial generation.
    Active,
    /// A complete generation no longer contains this cluster.
    Retired,
}

/// Current state of one generated cluster membership.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterMemberState {
    /// Included in the visible generated membership.
    Active,
    /// Excluded by a local maintainer decision.
    Excluded,
    /// No longer in a complete regenerated component.
    Removed,
}

/// Effective display role of one current cluster member.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterMemberRole {
    /// Selected by an explicit local canonical decision.
    Canonical,
    /// Selected by the generated graph representative rule.
    Representative,
    /// A related cluster member.
    Related,
}

/// Summary of one persisted generated cluster.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClusterSummary {
    /// Stable archive-local public cluster ID.
    pub id: u64,
    /// Current repository identity and display metadata.
    pub repository: Repository,
    /// Display title from the graph-selected representative.
    pub title: String,
    /// Active or retired generated state.
    pub lifecycle: ClusterLifecycle,
    /// Whether a maintainer locally dismissed this cluster.
    pub dismissed: bool,
    /// Reason supplied for a local dismissal, when dismissed.
    pub dismissal_reason: Option<String>,
    /// Effective canonical or generated representative.
    pub representative: Option<ThreadReference>,
    /// Number of active, non-excluded generated members.
    pub active_member_count: u64,
    /// Number of locally excluded members still present in this cluster.
    pub excluded_member_count: u64,
    /// Latest cluster generation that updated this identity.
    pub last_run_id: Option<u64>,
    /// Latest generation or local decision time.
    pub updated_at: UtcTimestamp,
}

/// A generated member with current local decision state.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ClusterMember {
    /// Current discussion and its evidence coverage.
    pub summary: ThreadSummary,
    /// Effective display role after local canonical selection.
    pub role: ClusterMemberRole,
    /// Current membership state.
    pub state: ClusterMemberState,
    /// Generated score relative to the graph-selected representative.
    pub score_to_representative: Option<f64>,
}

/// A persisted cluster and its current or locally excluded members.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ClusterDetail {
    /// Public cluster summary.
    pub cluster: ClusterSummary,
    /// Current active or excluded members in stable number order.
    pub members: Vec<ClusterMember>,
}

/// Repository scope and pagination for generated cluster listings.
pub struct ClusterListQuery<'a> {
    /// Resolved repository identities; an empty slice includes every repository.
    pub repositories: &'a [RepositoryId],
    /// Include clusters retired by a complete generation.
    pub include_retired: bool,
    /// Maximum number of rows, from 1 through 1000.
    pub limit: NonZeroU32,
    /// Number of matching clusters to skip.
    pub offset: u64,
}

/// One page of cluster summaries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClusterPage {
    /// Cluster summaries in stable size and ID order.
    pub items: Vec<ClusterSummary>,
    /// Offset to pass to the next request, when another page is available.
    pub next_offset: Option<u64>,
}

struct PreparedCluster {
    representative_id: i64,
    title: String,
    members: Vec<(i64, Option<f64>)>,
}

struct ExistingCluster {
    id: i64,
    members: HashSet<i64>,
}

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

    /// Lists durable generated clusters without contacting GitHub or mutating the archive.
    pub async fn list_clusters(
        &self,
        query: &ClusterListQuery<'_>,
    ) -> Result<ClusterPage, StoreError> {
        let limit = query.limit.get();
        if limit > 1000 || query.offset > i64::MAX as u64 {
            return Err(StoreError::InvalidClusterGeneration);
        }
        let mut statement = QueryBuilder::<Sqlite>::new(cluster_summary_select());
        if !query.include_retired {
            statement.push(" AND cg.status = 'active'");
        }
        push_cluster_repository_filter(&mut statement, query.repositories);
        statement
            .push(" ORDER BY active_member_count DESC, cg.id LIMIT ")
            .push_bind(i64::from(limit) + 1)
            .push(" OFFSET ")
            .push_bind(i64::try_from(query.offset).map_err(|_| StoreError::IntegerOutOfRange)?);
        let rows = statement.build().fetch_all(&self.reader).await?;
        let mut items = rows
            .into_iter()
            .map(cluster_summary_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = items.len() > usize::try_from(limit).unwrap_or(usize::MAX);
        items.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        let next_offset = if has_more {
            query
                .offset
                .checked_add(u64::try_from(items.len()).unwrap_or(u64::MAX))
        } else {
            None
        };
        Ok(ClusterPage { items, next_offset })
    }

    /// Shows a cluster and current or locally excluded generated members.
    pub async fn cluster_detail(&self, id: u64) -> Result<ClusterDetail, StoreError> {
        let cluster_id = checked_cluster_id(id)?;
        let mut statement = QueryBuilder::<Sqlite>::new(cluster_summary_select());
        statement.push(" AND cg.id = ").push_bind(cluster_id);
        let summary_row = statement
            .build()
            .fetch_optional(&self.reader)
            .await?
            .ok_or(StoreError::ClusterMissing)?;
        let canonical_thread_id: Option<i64> = summary_row.try_get("canonical_thread_id")?;
        let cluster = cluster_summary_from_row(summary_row)?;
        let rows = sqlx::query(
            "SELECT cm.thread_id, cm.state, cm.score_to_representative, t.payload_json AS discussion_json, r.payload_json AS repository_json FROM cluster_memberships cm JOIN threads t ON t.id = cm.thread_id JOIN repositories r ON r.id = t.repository_id WHERE cm.cluster_id = ? AND cm.state IN ('active', 'excluded') ORDER BY t.number, t.id",
        )
        .bind(cluster_id)
        .fetch_all(&self.reader)
        .await?;
        let thread_ids = rows
            .iter()
            .map(|row| row.try_get::<i64, _>("thread_id"))
            .collect::<Result<Vec<_>, _>>()?;
        let coverage = load_thread_coverage(&self.reader, &thread_ids).await?;
        let mut members = Vec::with_capacity(rows.len());
        for row in rows {
            let thread_id: i64 = row.try_get("thread_id")?;
            let state = parse_member_state(row.try_get::<String, _>("state")?.as_str())?;
            let discussion = serde_json::from_str(&row.try_get::<String, _>("discussion_json")?)?;
            let repository = serde_json::from_str(&row.try_get::<String, _>("repository_json")?)?;
            let summary = ThreadSummary {
                coverage: coverage_for_kind(&discussion, coverage.get(&thread_id)),
                discussion,
                repository,
            };
            let role = if canonical_thread_id == Some(thread_id) {
                ClusterMemberRole::Canonical
            } else if cluster
                .representative
                .as_ref()
                .is_some_and(|reference| reference.number() == summary.discussion.id.number())
            {
                ClusterMemberRole::Representative
            } else {
                ClusterMemberRole::Related
            };
            members.push(ClusterMember {
                summary,
                role,
                state,
                score_to_representative: row.try_get("score_to_representative")?,
            });
        }
        Ok(ClusterDetail { cluster, members })
    }

    /// Dismisses or restores a generated cluster as a local maintainer decision.
    pub async fn set_cluster_dismissed_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        dismissed: bool,
        reason: &str,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        if reason.len() > 2048 {
            return Err(StoreError::InvalidClusterGeneration);
        }
        let cluster_id = checked_cluster_id(id)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let event = if dismissed { "dismissed" } else { "restored" };
        let result = if dismissed {
            sqlx::query("UPDATE clusters SET dismissed_at_us = ?, dismissal_reason = ?, updated_at_us = ? WHERE id = ?")
                .bind(at.unix_microseconds())
                .bind(reason.trim())
                .bind(at.unix_microseconds())
                .bind(cluster_id)
                .execute(&mut *transaction)
                .await?
        } else {
            sqlx::query("UPDATE clusters SET dismissed_at_us = NULL, dismissal_reason = '', updated_at_us = ? WHERE id = ?")
                .bind(at.unix_microseconds())
                .bind(cluster_id)
                .execute(&mut *transaction)
                .await?
        };
        if result.rows_affected() != 1 {
            return Err(StoreError::ClusterMissing);
        }
        insert_cluster_event(
            &mut transaction,
            cluster_id,
            None,
            event,
            None,
            reason.trim(),
            at,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Excludes or includes one current generated member as a local decision.
    pub async fn set_cluster_member_excluded_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        thread: &ThreadId,
        excluded: bool,
        reason: &str,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        if reason.len() > 2048 {
            return Err(StoreError::InvalidClusterGeneration);
        }
        let cluster_id = checked_cluster_id(id)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let member_id = current_cluster_member_id(&mut transaction, cluster_id, thread).await?;
        sqlx::query(
            "INSERT INTO cluster_member_decisions (cluster_id, thread_id, excluded, reason, updated_at_us) VALUES (?, ?, ?, ?, ?) ON CONFLICT (cluster_id, thread_id) DO UPDATE SET excluded = excluded.excluded, reason = excluded.reason, updated_at_us = excluded.updated_at_us",
        )
        .bind(cluster_id)
        .bind(member_id)
        .bind(if excluded { 1_i64 } else { 0_i64 })
        .bind(reason.trim())
        .bind(at.unix_microseconds())
        .execute(&mut *transaction)
        .await?;
        let state = if excluded { "excluded" } else { "active" };
        sqlx::query("UPDATE cluster_memberships SET state = ?, updated_at_us = ? WHERE cluster_id = ? AND thread_id = ?")
            .bind(state)
            .bind(at.unix_microseconds())
            .bind(cluster_id)
            .bind(member_id)
            .execute(&mut *transaction)
            .await?;
        if excluded {
            sqlx::query("UPDATE clusters SET canonical_thread_id = NULL, updated_at_us = ? WHERE id = ? AND canonical_thread_id = ?")
                .bind(at.unix_microseconds())
                .bind(cluster_id)
                .bind(member_id)
                .execute(&mut *transaction)
                .await?;
        }
        let event = if excluded {
            "member_excluded"
        } else {
            "member_included"
        };
        insert_cluster_event(
            &mut transaction,
            cluster_id,
            None,
            event,
            Some(member_id),
            reason.trim(),
            at,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Sets the canonical member while preserving the generated representative for future runs.
    pub async fn set_cluster_canonical_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        thread: &ThreadId,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        let cluster_id = checked_cluster_id(id)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let member_id = current_cluster_member_id(&mut transaction, cluster_id, thread).await?;
        let state: String = sqlx::query_scalar(
            "SELECT state FROM cluster_memberships WHERE cluster_id = ? AND thread_id = ?",
        )
        .bind(cluster_id)
        .bind(member_id)
        .fetch_one(&mut *transaction)
        .await?;
        if state != "active" {
            return Err(StoreError::ClusterMemberMissing);
        }
        let result = sqlx::query(
            "UPDATE clusters SET canonical_thread_id = ?, updated_at_us = ? WHERE id = ?",
        )
        .bind(member_id)
        .bind(at.unix_microseconds())
        .bind(cluster_id)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::ClusterMissing);
        }
        insert_cluster_event(
            &mut transaction,
            cluster_id,
            None,
            "canonical_set",
            Some(member_id),
            "",
            at,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }
}

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

async fn thread_row_id(
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

async fn insert_cluster_event(
    connection: &mut SqliteConnection,
    cluster_id: i64,
    run_id: Option<i64>,
    event_type: &str,
    thread_id: Option<i64>,
    reason: &str,
    at: UtcTimestamp,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO cluster_events (cluster_id, run_id, event_type, thread_id, reason, created_at_us) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(cluster_id)
    .bind(run_id)
    .bind(event_type)
    .bind(thread_id)
    .bind(reason)
    .bind(at.unix_microseconds())
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn current_cluster_member_id(
    connection: &mut SqliteConnection,
    cluster_id: i64,
    thread: &ThreadId,
) -> Result<i64, StoreError> {
    let cluster_repository: Option<(String, String)> = sqlx::query_as(
        "SELECT r.host, r.provider_id FROM clusters c JOIN repositories r ON r.id = c.repository_id WHERE c.id = ?",
    )
    .bind(cluster_id)
    .fetch_optional(&mut *connection)
    .await?;
    let (host, repository_provider_id) = cluster_repository.ok_or(StoreError::ClusterMissing)?;
    if host != thread.repository().host().as_str()
        || repository_provider_id != thread.repository().provider_id().as_str()
    {
        return Err(StoreError::ClusterMemberMissing);
    }
    let repository_id: i64 = sqlx::query_scalar("SELECT repository_id FROM clusters WHERE id = ?")
        .bind(cluster_id)
        .fetch_one(&mut *connection)
        .await?;
    let thread_id = thread_row_id(connection, repository_id, thread).await?;
    let membership: Option<String> = sqlx::query_scalar(
        "SELECT state FROM cluster_memberships WHERE cluster_id = ? AND thread_id = ?",
    )
    .bind(cluster_id)
    .bind(thread_id)
    .fetch_optional(&mut *connection)
    .await?;
    if membership.is_none_or(|state| state == "removed") {
        return Err(StoreError::ClusterMemberMissing);
    }
    Ok(thread_id)
}

fn checked_cluster_id(id: u64) -> Result<i64, StoreError> {
    i64::try_from(id)
        .ok()
        .filter(|id| *id > 0)
        .ok_or(StoreError::ClusterMissing)
}

fn cluster_summary_select() -> &'static str {
    "SELECT cg.id, r.payload_json AS repository_json, cg.title, cg.status, cg.dismissed_at_us, cg.dismissal_reason, cg.last_run_id, cg.updated_at_us, cg.canonical_thread_id, cg.representative_thread_id, COALESCE((SELECT t.number FROM threads t JOIN cluster_memberships cm ON cm.thread_id = t.id WHERE cm.cluster_id = cg.id AND cm.thread_id = cg.canonical_thread_id AND cm.state = 'active'), (SELECT t.number FROM threads t JOIN cluster_memberships cm ON cm.thread_id = t.id WHERE cm.cluster_id = cg.id AND cm.thread_id = cg.representative_thread_id AND cm.state = 'active'), (SELECT t.number FROM threads t JOIN cluster_memberships cm ON cm.thread_id = t.id WHERE cm.cluster_id = cg.id AND cm.state = 'active' ORDER BY t.number, t.id LIMIT 1)) AS representative_number, (SELECT COUNT(*) FROM cluster_memberships cm WHERE cm.cluster_id = cg.id AND cm.state = 'active') AS active_member_count, (SELECT COUNT(*) FROM cluster_memberships cm WHERE cm.cluster_id = cg.id AND cm.state = 'excluded') AS excluded_member_count FROM clusters cg JOIN repositories r ON r.id = cg.repository_id WHERE 1 = 1"
}

fn push_cluster_repository_filter(
    statement: &mut QueryBuilder<Sqlite>,
    repositories: &[RepositoryId],
) {
    if repositories.is_empty() {
        return;
    }
    statement.push(" AND (");
    for (index, repository) in repositories.iter().enumerate() {
        if index > 0 {
            statement.push(" OR ");
        }
        statement
            .push("(r.host = ")
            .push_bind(repository.host().as_str())
            .push(" AND r.provider_id = ")
            .push_bind(repository.provider_id().as_str())
            .push(")");
    }
    statement.push(")");
}

fn cluster_summary_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ClusterSummary, StoreError> {
    let repository: Repository =
        serde_json::from_str(&row.try_get::<String, _>("repository_json")?)?;
    let representative_number: Option<i64> = row.try_get("representative_number")?;
    let representative = representative_number
        .map(|number| -> Result<ThreadReference, StoreError> {
            let number = u64::try_from(number).map_err(|_| StoreError::InvalidStoredCount)?;
            let number = ThreadNumber::new(number).map_err(|_| StoreError::InvalidStoredCount)?;
            Ok(ThreadReference::new(repository.id.clone(), number))
        })
        .transpose()?;
    let id =
        u64::try_from(row.try_get::<i64, _>("id")?).map_err(|_| StoreError::InvalidStoredCount)?;
    let last_run_id = row
        .try_get::<Option<i64>, _>("last_run_id")?
        .map(|id| u64::try_from(id).map_err(|_| StoreError::InvalidStoredCount))
        .transpose()?;
    let updated_at = UtcTimestamp::from_unix_microseconds(row.try_get("updated_at_us")?)
        .map_err(StoreError::InvalidCreatedAt)?;
    let lifecycle = match row.try_get::<String, _>("status")?.as_str() {
        "active" => ClusterLifecycle::Active,
        "retired" => ClusterLifecycle::Retired,
        _ => return Err(StoreError::InvalidClusterGeneration),
    };
    let dismissed = row.try_get::<Option<i64>, _>("dismissed_at_us")?.is_some();
    let dismissal_reason: String = row.try_get("dismissal_reason")?;
    Ok(ClusterSummary {
        id,
        repository,
        title: row.try_get("title")?,
        lifecycle,
        dismissed,
        dismissal_reason: dismissed.then_some(dismissal_reason),
        representative,
        active_member_count: u64::try_from(row.try_get::<i64, _>("active_member_count")?)
            .map_err(|_| StoreError::InvalidStoredCount)?,
        excluded_member_count: u64::try_from(row.try_get::<i64, _>("excluded_member_count")?)
            .map_err(|_| StoreError::InvalidStoredCount)?,
        last_run_id,
        updated_at,
    })
}

fn parse_member_state(value: &str) -> Result<ClusterMemberState, StoreError> {
    match value {
        "active" => Ok(ClusterMemberState::Active),
        "excluded" => Ok(ClusterMemberState::Excluded),
        "removed" => Ok(ClusterMemberState::Removed),
        _ => Err(StoreError::InvalidClusterGeneration),
    }
}
