//! # Read clusters for inspection and triage
//!
//! These `Archive` methods assemble cluster summaries, membership, and details from stored
//! generations and local decisions. They return projection types declared in the parent module,
//! leaving SQL row shapes private.
//!
//! Use this path for offline list/show operations in the engine, CLI, and TUI. Queries must
//! reflect member roles and lifecycle state; callers should not reconstruct those semantics by
//! joining raw tables themselves.
//!
//! Summary SQL chooses an effective representative from active members: the local canonical
//! choice wins, followed by the generated representative, then the lowest thread number. A cluster
//! without active members has no effective representative. Titles remain generation-derived.
//!
//! List order is descending active membership count followed by stable cluster ID. Detail members
//! are ordered by thread number and archive row ID; excluded members remain visible, while removed
//! members are omitted. Canonical, representative, and related roles are separate from member
//! state.
//!
//! Summary, membership, and coverage are separate reads. Concurrent writers may advance between
//! them, so detail is an inspection projection rather than a frozen generation snapshot.

use sqlx::Row;

use super::{
    Archive, ClusterDetail, ClusterLifecycle, ClusterListQuery, ClusterMember, ClusterMemberRole,
    ClusterMemberState, ClusterPage, ClusterSummary, QueryBuilder, Repository, RepositoryId,
    Sqlite, StoreError, ThreadNumber, ThreadReference, ThreadSummary, UtcTimestamp,
    checked_cluster_id, coverage_for_kind, load_thread_coverage,
};

impl Archive {
    /// Lists durable generated clusters without contacting GitHub or mutating the archive.
    ///
    /// Repository scope is optional. Retired clusters are omitted unless requested; dismissal is
    /// an independent local state and does not itself remove a cluster from this list. Results use
    /// active-member count followed by stable ID, fetching one extra row for continuation
    /// detection.
    ///
    /// # Errors
    ///
    /// Rejects page sizes above 1,000 and offsets outside SQLite's signed range. Invalid persisted
    /// identities, counts, timestamps, lifecycle labels, payloads, and SQL failures return typed
    /// archive errors; no query changes membership or local decisions.
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
    ///
    /// Removed members are omitted. Each selected member includes its canonical discussion and
    /// explicit evidence coverage. Local canonical selection takes precedence over representative
    /// role; exclusions are reported as member state rather than dropping the discussion.
    ///
    /// Summary, member rows, and coverage are acquired separately. A concurrent generation or
    /// decision can advance between those reads; callers must not treat this view as a transaction
    /// snapshot or use it as authority to bypass validation in a later decision operation.
    ///
    /// # Errors
    ///
    /// Invalid or absent cluster IDs return `ClusterMissing`. Malformed stored rows and SQL
    /// failures abort projection without changing archive state.
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
}

/// Defines the common projection used by cluster list and detail reads.
fn cluster_summary_select() -> &'static str {
    "SELECT cg.id, r.payload_json AS repository_json, cg.title, cg.status, cg.dismissed_at_us, cg.dismissal_reason, cg.last_run_id, cg.updated_at_us, cg.canonical_thread_id, cg.representative_thread_id, COALESCE((SELECT t.number FROM threads t JOIN cluster_memberships cm ON cm.thread_id = t.id WHERE cm.cluster_id = cg.id AND cm.thread_id = cg.canonical_thread_id AND cm.state = 'active'), (SELECT t.number FROM threads t JOIN cluster_memberships cm ON cm.thread_id = t.id WHERE cm.cluster_id = cg.id AND cm.thread_id = cg.representative_thread_id AND cm.state = 'active'), (SELECT t.number FROM threads t JOIN cluster_memberships cm ON cm.thread_id = t.id WHERE cm.cluster_id = cg.id AND cm.state = 'active' ORDER BY t.number, t.id LIMIT 1)) AS representative_number, (SELECT COUNT(*) FROM cluster_memberships cm WHERE cm.cluster_id = cg.id AND cm.state = 'active') AS active_member_count, (SELECT COUNT(*) FROM cluster_memberships cm WHERE cm.cluster_id = cg.id AND cm.state = 'excluded') AS excluded_member_count FROM clusters cg JOIN repositories r ON r.id = cg.repository_id WHERE 1 = 1"
}

/// Adds bound repository scope to a cluster query.
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

/// Converts a SQL projection to a typed cluster summary.
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

/// Rejects unknown stored member-decision labels.
fn parse_member_state(value: &str) -> Result<ClusterMemberState, StoreError> {
    match value {
        "active" => Ok(ClusterMemberState::Active),
        "excluded" => Ok(ClusterMemberState::Excluded),
        "removed" => Ok(ClusterMemberState::Removed),
        _ => Err(StoreError::InvalidClusterGeneration),
    }
}
