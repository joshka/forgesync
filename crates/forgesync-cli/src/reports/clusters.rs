//! Clusters command presentation.

use super::*;

pub(crate) fn cluster_build_summary(report: &ClusterBuildReport) -> String {
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

pub(crate) fn cluster_page_summary(page: &ClusterPage) -> String {
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

pub(crate) fn cluster_detail_summary(detail: &ClusterDetail) -> String {
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

pub(crate) fn cluster_decision_summary(output: &ClusterDecisionOutput) -> String {
    format!("Cluster #{}: {}", output.cluster_id, output.action)
}
