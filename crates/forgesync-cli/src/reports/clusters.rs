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
use forgesync_store::clusters::{ClusterDetail, ClusterPage};
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
    for cluster in &page.items {
        let lifecycle = match cluster.lifecycle {
            forgesync_store::clusters::ClusterLifecycle::Active => "active",
            forgesync_store::clusters::ClusterLifecycle::Retired => "retired",
        };
        let dismissed = if cluster.dismissed { ", dismissed" } else { "" };
        lines.push(format!(
            "#{} [{}{}] {} active / {} excluded: {}",
            cluster.id,
            lifecycle,
            dismissed,
            cluster.active_member_count,
            cluster.excluded_member_count,
            cluster.title
        ));
    }
    if let Some(offset) = page.next_offset {
        lines.push(format!("Next page: --offset {offset}"));
    }
    lines.join("\n")
}

/// Formats a cluster and its member decisions without mutating them.
pub fn cluster_detail_summary(detail: &ClusterDetail) -> String {
    let cluster = &detail.cluster;
    let lifecycle = match cluster.lifecycle {
        forgesync_store::clusters::ClusterLifecycle::Active => "active",
        forgesync_store::clusters::ClusterLifecycle::Retired => "retired",
    };
    let representative = cluster.representative.as_ref().map_or_else(
        || "none".to_owned(),
        |thread| format!("#{}", thread.number().get()),
    );
    let mut lines = vec![format!(
        "Cluster #{} [{}] {} — {} ({} members, representative {})",
        cluster.id,
        lifecycle,
        cluster.repository.full_name,
        cluster.title,
        cluster.active_member_count + cluster.excluded_member_count,
        representative
    )];
    if cluster.dismissed {
        lines.push(format!(
            "Dismissed: {}",
            cluster.dismissal_reason.as_deref().unwrap_or_default()
        ));
    }
    for member in &detail.members {
        let role = match member.role {
            forgesync_store::clusters::ClusterMemberRole::Canonical => "canonical",
            forgesync_store::clusters::ClusterMemberRole::Representative => "representative",
            forgesync_store::clusters::ClusterMemberRole::Related => "related",
        };
        let state = match member.state {
            forgesync_store::clusters::ClusterMemberState::Active => "active",
            forgesync_store::clusters::ClusterMemberState::Excluded => "excluded",
            forgesync_store::clusters::ClusterMemberState::Removed => "removed",
        };
        lines.push(format!(
            "  #{} [{role}, {state}] {}",
            member.summary.discussion.id.number().get(),
            member.summary.discussion.title
        ));
    }
    lines.join("\n")
}

/// Confirms the local maintainer action recorded for a cluster.
pub fn cluster_decision_summary(output: &ClusterDecisionOutput) -> String {
    format!("Cluster #{}: {}", output.cluster_id, output.action)
}

#[cfg(test)]
mod tests {
    //! Small command DTOs retain their serialized field contract when ownership changes.

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
