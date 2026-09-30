//! # Explain cluster generations and triage
//!
//! These summaries present build counts, list pages, cluster detail, and recorded maintainer
//! decisions. Their input types come from the engine and store projections, not raw SQL.
//!
//! A generated suggestion and a local decision have different meanings. Terminal wording should
//! preserve that distinction so a reader knows whether they are seeing analysis output or an
//! explicit choice.
//!
//! [`ClusterDecisionOutput`] identifies a successfully recorded action for command JSON and human
//! confirmation. Build/page/detail functions consume their engine/store reports directly. They
//! format local evidence and decisions without sending GitHub write-back or recomputing clusters.

use forgesync_engine::clustering::ClusterBuildReport;
use forgesync_store::clusters::{
    ClusterDetail, ClusterLifecycle, ClusterMember, ClusterMemberRole, ClusterMemberState,
    ClusterPage, ClusterSummary,
};
use serde::Serialize;

/// Recorded local maintainer action, separate from a generated cluster suggestion.
#[derive(Debug, Serialize)]
pub struct ClusterDecisionOutput {
    /// Archive-local cluster identity affected by the successful decision.
    pub cluster_id: u64,
    /// Stable action label chosen by the command, such as dismiss, restore, or exclude.
    pub action: &'static str,
}

/// Summarizes the generated cluster count and coverage for human output.
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

/// Formats one page of cluster summaries for terminal inspection.
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

/// Formats a cluster and its member decisions without mutating them.
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

/// Shows lifecycle, dismissal, and inclusion counts as distinct facts in one list row.
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

/// Identifies the cluster and effective representative before its current member decisions.
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

/// Presents membership role independently of its active/excluded/removed state.
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

/// Names generation lifecycle without conflating it with local dismissal.
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

/// Keeps inclusion-state labels independent of representative/canonical role.
fn member_state_name(state: ClusterMemberState) -> &'static str {
    match state {
        ClusterMemberState::Active => "active",
        ClusterMemberState::Excluded => "excluded",
        ClusterMemberState::Removed => "removed",
    }
}

/// Confirms the local maintainer action recorded for a cluster.
pub fn cluster_decision_summary(output: &ClusterDecisionOutput) -> String {
    format!("Cluster #{}: {}", output.cluster_id, output.action)
}

#[cfg(test)]
mod tests {
    //! # Empty clusters and decision acknowledgment projections
    //!
    //! These cases construct an empty stored page and a command acknowledgment directly.
    //! An empty page has explicit no-results wording; a decision retains local cluster ID and
    //! action. Whole-value JSON comparison protects those acknowledgment fields from accidental
    //! changes.
    //!
    //! This projection does not apply the decision or verify that the target exists.
    //! Command process cases establish mutation/error dispatch; store tests establish persistence.
    //! Static values isolate representation from graph construction and provider acquisition.
    //! The two small contracts stay inline beside their presentation owner.

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
