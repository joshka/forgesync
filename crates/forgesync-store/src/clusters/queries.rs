//! Cluster list and detail reads.
//!
//! The effective representative prefers an active local canonical member, then the generated
//! representative, then the lowest active thread number. Detail members are ordered by number;
//! excluded members stay visible while removed ones are omitted. Summary, members, and coverage are
//! separate reads, so detail is not a frozen snapshot.

use std::collections::HashMap;

use forgesync_core::content::Repository;
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::{ThreadNumber, ThreadReference};
use sqlx::sqlite::SqliteRow;
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::archive::Archive;
use crate::clusters::decisions::checked_cluster_id;
use crate::clusters::{
    ClusterDetail, ClusterLifecycle, ClusterListQuery, ClusterMember, ClusterMemberRole,
    ClusterMemberState, ClusterPage, ClusterSummary,
};
use crate::coverage_projection::{StoredCoverage, coverage_for_kind, load_thread_coverage};
use crate::error::StoreError;
use crate::reads::ThreadSummary;
use crate::sql::{count_from_sql, push_repository_scope, timestamp_from_sql, to_sql_integer};

impl Archive {
    /// Lists generated clusters by descending active-member count, then ID.
    ///
    /// Retired clusters are omitted unless requested; dismissal does not remove a cluster here.
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
        push_repository_scope(&mut statement, query.repositories);
        statement
            .push(" ORDER BY active_member_count DESC, cg.id LIMIT ")
            .push_bind(i64::from(limit) + 1)
            .push(" OFFSET ")
            .push_bind(to_sql_integer(query.offset)?);
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

    /// Shows a cluster with its active and locally excluded members.
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
        let roles = MemberRoles {
            canonical_thread_id,
            representative: cluster.representative.as_ref().map(ThreadReference::number),
        };
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
        let members = rows
            .into_iter()
            .map(|row| cluster_member(row, &roles, &coverage))
            .collect::<Result<_, _>>()?;
        Ok(ClusterDetail { cluster, members })
    }
}

/// Defines the common projection used by cluster list and detail reads.
fn cluster_summary_select() -> &'static str {
    "SELECT cg.id, r.payload_json AS repository_json, cg.title, cg.status, cg.dismissed_at_us, cg.dismissal_reason, cg.last_run_id, cg.updated_at_us, cg.canonical_thread_id, cg.representative_thread_id, COALESCE((SELECT t.number FROM threads t JOIN cluster_memberships cm ON cm.thread_id = t.id WHERE cm.cluster_id = cg.id AND cm.thread_id = cg.canonical_thread_id AND cm.state = 'active'), (SELECT t.number FROM threads t JOIN cluster_memberships cm ON cm.thread_id = t.id WHERE cm.cluster_id = cg.id AND cm.thread_id = cg.representative_thread_id AND cm.state = 'active'), (SELECT t.number FROM threads t JOIN cluster_memberships cm ON cm.thread_id = t.id WHERE cm.cluster_id = cg.id AND cm.state = 'active' ORDER BY t.number, t.id LIMIT 1)) AS representative_number, (SELECT COUNT(*) FROM cluster_memberships cm WHERE cm.cluster_id = cg.id AND cm.state = 'active') AS active_member_count, (SELECT COUNT(*) FROM cluster_memberships cm WHERE cm.cluster_id = cg.id AND cm.state = 'excluded') AS excluded_member_count FROM clusters cg JOIN repositories r ON r.id = cg.repository_id WHERE 1 = 1"
}

/// Converts a SQL projection to a typed cluster summary.
fn cluster_summary_from_row(row: SqliteRow) -> Result<ClusterSummary, StoreError> {
    let repository: Repository =
        serde_json::from_str(&row.try_get::<String, _>("repository_json")?)?;
    let representative_number: Option<i64> = row.try_get("representative_number")?;
    let representative = representative_number
        .map(|number| -> Result<ThreadReference, StoreError> {
            let number = ThreadNumber::new(count_from_sql(number)?)
                .map_err(|_| StoreError::Corrupt("archive_count_invalid"))?;
            Ok(ThreadReference::new(repository.id.clone(), number))
        })
        .transpose()?;
    let id = count_from_sql(row.try_get("id")?)?;
    let last_run_id = row
        .try_get::<Option<i64>, _>("last_run_id")?
        .map(count_from_sql)
        .transpose()?;
    let updated_at = timestamp_from_sql(row.try_get("updated_at_us")?)?;
    let lifecycle: ClusterLifecycle = row.try_get("status")?;
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
        active_member_count: count_from_sql(row.try_get("active_member_count")?)?,
        excluded_member_count: count_from_sql(row.try_get("excluded_member_count")?)?,
        last_run_id,
        updated_at,
    })
}

/// Role coordinates selected by the cluster-summary read.
struct MemberRoles {
    /// Local canonical choice expressed as an archive thread row ID.
    canonical_thread_id: Option<i64>,
    /// Effective display representative expressed as a repository thread number.
    representative: Option<ThreadNumber>,
}

/// Decodes one member row and attaches its coverage and effective role.
fn cluster_member(
    row: SqliteRow,
    roles: &MemberRoles,
    coverage: &HashMap<i64, HashMap<EvidenceFamily, StoredCoverage>>,
) -> Result<ClusterMember, StoreError> {
    let thread_id: i64 = row.try_get("thread_id")?;
    let state: ClusterMemberState = row.try_get("state")?;
    let discussion = serde_json::from_str(&row.try_get::<String, _>("discussion_json")?)?;
    let repository = serde_json::from_str(&row.try_get::<String, _>("repository_json")?)?;
    let summary = ThreadSummary {
        coverage: coverage_for_kind(&discussion, coverage.get(&thread_id)),
        discussion,
        repository,
    };
    let role = roles.role(thread_id, summary.discussion.id.number());
    Ok(ClusterMember {
        summary,
        role,
        state,
        score_to_representative: row.try_get("score_to_representative")?,
    })
}

impl MemberRoles {
    /// Applies canonical precedence without conflating archive IDs and repository numbers.
    fn role(&self, thread_id: i64, number: ThreadNumber) -> ClusterMemberRole {
        if self.canonical_thread_id == Some(thread_id) {
            ClusterMemberRole::Canonical
        } else if self.representative == Some(number) {
            ClusterMemberRole::Representative
        } else {
            ClusterMemberRole::Related
        }
    }
}

#[cfg(test)]
mod tests {
    use forgesync_core::identity::ThreadNumber;

    use crate::clusters::ClusterMemberRole;
    use crate::clusters::queries::MemberRoles;

    #[test]
    fn canonical_role_wins_when_the_same_thread_is_the_representative() {
        let number = ThreadNumber::new(17).expect("thread number");
        let roles = MemberRoles {
            canonical_thread_id: Some(41),
            representative: Some(number),
        };
        assert_eq!(roles.role(41, number), ClusterMemberRole::Canonical);
    }

    #[test]
    fn representative_uses_repository_number_not_archive_row_identity() {
        let number = ThreadNumber::new(17).expect("thread number");
        let roles = MemberRoles {
            canonical_thread_id: None,
            representative: Some(number),
        };
        assert_eq!(roles.role(41, number), ClusterMemberRole::Representative);
    }

    #[test]
    fn a_member_without_a_selected_identity_is_related() {
        let number = ThreadNumber::new(17).expect("thread number");
        let roles = MemberRoles {
            canonical_thread_id: None,
            representative: None,
        };
        assert_eq!(roles.role(41, number), ClusterMemberRole::Related);
    }
}
