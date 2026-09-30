//! # Enrich selected cluster membership with discussion evidence
//!
//! `load_members` reads active/excluded membership in thread-number order, then loads explicit
//! evidence coverage for those discussions. Removed rows are not part of the detail projection.
//! These are separate reads and retain the inspection consistency contract of `cluster_detail`.
//!
//! `MemberRoles` keeps canonical archive identity separate from effective representative number.
//! Canonical role wins, then representative, then related. The effective representative has already
//! been selected by summary SQL; this module does not repeat its fallback queries.
//!
//! `MemberProjection` decodes each SQL row and attaches coverage and role. Invalid stored state,
//! payloads, or scores fail the complete projection rather than returning a partially decoded list.

use std::collections::HashMap;

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::ThreadNumber;
use sqlx::{Row, SqlitePool};

use crate::clusters::{ClusterMember, ClusterMemberRole, ClusterMemberState};
use crate::error::StoreError;
use crate::reads::{StoredCoverage, ThreadSummary, coverage_for_kind, load_thread_coverage};

/// Role coordinates selected by the cluster-summary read.
pub struct MemberRoles {
    /// Local canonical choice expressed as an archive thread row ID.
    pub canonical_thread_id: Option<i64>,
    /// Effective display representative expressed as a repository thread number.
    pub representative: Option<ThreadNumber>,
}

/// Reads selected membership and enriches it without altering archive state.
pub async fn load_members(
    pool: &SqlitePool,
    cluster_id: i64,
    roles: MemberRoles,
) -> Result<Vec<ClusterMember>, StoreError> {
    let rows = sqlx::query(
            "SELECT cm.thread_id, cm.state, cm.score_to_representative, t.payload_json AS discussion_json, r.payload_json AS repository_json FROM cluster_memberships cm JOIN threads t ON t.id = cm.thread_id JOIN repositories r ON r.id = t.repository_id WHERE cm.cluster_id = ? AND cm.state IN ('active', 'excluded') ORDER BY t.number, t.id",
        )
        .bind(cluster_id)
        .fetch_all(pool)
        .await?;
    let thread_ids = rows
        .iter()
        .map(|row| row.try_get::<i64, _>("thread_id"))
        .collect::<Result<Vec<_>, _>>()?;
    let coverage = load_thread_coverage(pool, &thread_ids).await?;
    let projection = MemberProjection {
        roles,
        coverage: &coverage,
    };
    rows.into_iter().map(|row| projection.member(row)).collect()
}

/// Shared role/coverage interpretation used for every row of one detail read.
struct MemberProjection<'a> {
    /// Effective canonical and representative roles from the earlier summary query.
    roles: MemberRoles,
    /// Thread-keyed family evidence from the subsequent coverage query.
    coverage: &'a HashMap<i64, HashMap<EvidenceFamily, StoredCoverage>>,
}

impl MemberProjection<'_> {
    /// Decodes one member and keeps its discussion, evidence, role, and state together.
    fn member(&self, row: sqlx::sqlite::SqliteRow) -> Result<ClusterMember, StoreError> {
        let thread_id: i64 = row.try_get("thread_id")?;
        let state = parse_member_state(row.try_get::<String, _>("state")?.as_str())?;
        let discussion = serde_json::from_str(&row.try_get::<String, _>("discussion_json")?)?;
        let repository = serde_json::from_str(&row.try_get::<String, _>("repository_json")?)?;
        let summary = ThreadSummary {
            coverage: coverage_for_kind(&discussion, self.coverage.get(&thread_id)),
            discussion,
            repository,
        };
        let role = self.roles.role(thread_id, summary.discussion.id.number());
        Ok(ClusterMember {
            summary,
            role,
            state,
            score_to_representative: row.try_get("score_to_representative")?,
        })
    }
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

/// Rejects unknown stored member-decision labels.
fn parse_member_state(value: &str) -> Result<ClusterMemberState, StoreError> {
    match value {
        "active" => Ok(ClusterMemberState::Active),
        "excluded" => Ok(ClusterMemberState::Excluded),
        "removed" => Ok(ClusterMemberState::Removed),
        _ => Err(StoreError::InvalidClusterGeneration),
    }
}

#[cfg(test)]
mod tests {
    use forgesync_core::identity::ThreadNumber;

    use crate::clusters::ClusterMemberRole;
    use crate::clusters::members::MemberRoles;

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
