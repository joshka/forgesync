//! Cluster generation, list, detail, and decision summaries.
//!
//! A generated suggestion and a local maintainer decision have different meanings; wording keeps
//! them distinguishable.

use forgesync_engine::clustering::ClusterBuildReport;
use forgesync_store::clusters::{
    ClusterDetail, ClusterLifecycle, ClusterMember, ClusterMemberRole, ClusterMemberState,
    ClusterPage, ClusterSummary,
};
use serde::Serialize;

/// Recorded local maintainer action, separate from a generated cluster suggestion.
#[derive(Debug, Serialize)]
pub struct ClusterDecisionOutput {
    pub cluster_id: u64,
    /// Past-tense action label, such as `dismissed` or `member_excluded`.
    pub action: &'static str,
}

/// Summarizes generated groups and whether vector coverage was complete.
pub fn cluster_build_summary(report: &ClusterBuildReport) -> String {
    let coverage = if report.generation.complete_coverage {
        "complete"
    } else {
        "partial"
    };
    format!(
        "Cluster build {coverage}: {} eligible discussions, {} with vectors, {} candidate edges, {} groups, {} members, {} groups retired (run {})",
        report.eligible_threads,
        report.vector_threads,
        report.candidate_edges,
        report.generation.cluster_count,
        report.generation.member_count,
        report.generation.retired_count,
        report.generation.run_id
    )
}

/// Lists one page of clusters, with a continuation hint when more exist.
pub fn cluster_page_summary(page: &ClusterPage) -> String {
    if page.items.is_empty() {
        return "No clusters found".to_owned();
    }
    let mut lines = Vec::with_capacity(page.items.len() + 1);
    lines.push(format!("{} cluster(s)", page.items.len()));
    lines.extend(page.items.iter().map(cluster_row));
    if let Some(offset) = page.next_offset {
        lines.push(format!("Next page: --offset {offset}"));
    }
    lines.join("\n")
}

/// Shows one cluster heading followed by its members and their local decisions.
pub fn cluster_detail_summary(detail: &ClusterDetail) -> String {
    let mut lines = vec![cluster_heading(&detail.cluster)];
    if detail.cluster.dismissed {
        lines.push(format!(
            "Dismissed: {}",
            detail
                .cluster
                .dismissal_reason
                .as_deref()
                .unwrap_or_default()
        ));
    }
    lines.extend(detail.members.iter().map(member_row));
    lines.join("\n")
}

/// Shows lifecycle, dismissal, and member counts as distinct facts in one row.
fn cluster_row(cluster: &ClusterSummary) -> String {
    let lifecycle = lifecycle_name(cluster.lifecycle);
    let dismissed = if cluster.dismissed { ", dismissed" } else { "" };
    format!(
        "#{} [{}{}] {} active / {} excluded: {}",
        cluster.id,
        lifecycle,
        dismissed,
        cluster.active_member_count,
        cluster.excluded_member_count,
        cluster.title
    )
}

/// Identifies the cluster and its effective representative.
fn cluster_heading(cluster: &ClusterSummary) -> String {
    let lifecycle = lifecycle_name(cluster.lifecycle);
    let representative = cluster.representative.as_ref().map_or_else(
        || "none".to_owned(),
        |thread| format!("#{}", thread.number().get()),
    );
    let members = cluster.active_member_count + cluster.excluded_member_count;
    format!(
        "Cluster #{} [{}] {} — {} ({} members, representative {})",
        cluster.id, lifecycle, cluster.repository.full_name, cluster.title, members, representative
    )
}

/// Shows one member with its role and inclusion state.
fn member_row(member: &ClusterMember) -> String {
    let role = member_role_name(member.role);
    let state = member_state_name(member.state);
    let discussion = &member.summary.discussion;
    format!(
        "  #{} [{role}, {state}] {}",
        discussion.id.number().get(),
        discussion.title
    )
}

/// Generation lifecycle, which is independent of local dismissal.
fn lifecycle_name(lifecycle: ClusterLifecycle) -> &'static str {
    match lifecycle {
        ClusterLifecycle::Active => "active",
        ClusterLifecycle::Retired => "retired",
    }
}

/// Distinguishes explicit canonical choice from generated representative and related roles.
fn member_role_name(role: ClusterMemberRole) -> &'static str {
    match role {
        ClusterMemberRole::Canonical => "canonical",
        ClusterMemberRole::Representative => "representative",
        ClusterMemberRole::Related => "related",
    }
}

/// Inclusion state, which is independent of the member's role.
fn member_state_name(state: ClusterMemberState) -> &'static str {
    match state {
        ClusterMemberState::Active => "active",
        ClusterMemberState::Excluded => "excluded",
        ClusterMemberState::Removed => "removed",
    }
}

/// Confirms the local decision recorded for a cluster.
pub fn cluster_decision_summary(output: &ClusterDecisionOutput) -> String {
    format!("Cluster #{}: {}", output.cluster_id, output.action)
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_page_uses_the_existing_no_clusters_message() {
        let page = forgesync_store::clusters::ClusterPage {
            items: Vec::new(),
            next_offset: None,
        };
        assert_eq!(
            crate::reports::clusters::cluster_page_summary(&page),
            "No clusters found"
        );
    }

    #[test]
    fn decision_json_retains_archive_identity_and_action() {
        let output = crate::reports::clusters::ClusterDecisionOutput {
            cluster_id: 17,
            action: "dismiss",
        };

        let json = serde_json::to_value(output).expect("decision JSON");

        assert_eq!(
            json,
            serde_json::json!({"cluster_id": 17, "action": "dismiss"})
        );
    }
}
