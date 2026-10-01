//! Fixed domain values for app and query tests.

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::clusters::{
    ClusterDetail, ClusterLifecycle, ClusterMember, ClusterMemberRole, ClusterMemberState,
    ClusterSummary,
};
use forgesync_store::reads::ThreadSummary;

use crate::app::loadable::Loadable;
use crate::app::panels::ClusterDetailPane;

pub fn sample_repository() -> Repository {
    Repository {
        id: RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new("41").expect("repository ID"),
        ),
        owner: "owner".to_owned(),
        name: "repo".to_owned(),
        full_name: "owner/repo".to_owned(),
        default_branch: Some("main".to_owned()),
        updated_at: None,
        provider_data: ProviderData::new(),
    }
}

/// Cluster 17 with one representative member, `owner/repo#7`.
pub fn sample_cluster_detail() -> ClusterDetail {
    let repository = sample_repository();
    let timestamp = UtcTimestamp::parse("2026-09-29T00:00:00Z").expect("timestamp");
    let discussion = Discussion {
        id: ThreadId::new(
            repository.id.clone(),
            ProviderId::new("1001").expect("thread ID"),
            ThreadNumber::new(7).expect("thread number"),
        ),
        kind: ThreadKind::Issue,
        state: SourceState::Open,
        title: "Selected neighbor".to_owned(),
        body: None,
        html_url: None,
        created_at: timestamp,
        updated_at: timestamp,
        closed_at: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        provider_data: ProviderData::new(),
    };
    ClusterDetail {
        cluster: ClusterSummary {
            id: 17,
            repository: repository.clone(),
            title: "Cluster title".to_owned(),
            lifecycle: ClusterLifecycle::Active,
            dismissed: false,
            dismissal_reason: None,
            representative: None,
            active_member_count: 1,
            excluded_member_count: 0,
            last_run_id: Some(2),
            updated_at: timestamp,
        },
        members: vec![ClusterMember {
            summary: ThreadSummary {
                repository,
                discussion,
                coverage: Vec::new(),
            },
            role: ClusterMemberRole::Representative,
            state: ClusterMemberState::Active,
            score_to_representative: Some(1.0),
        }],
    }
}

/// A cluster detail pane that has loaded `detail` and selected its first member.
pub fn loaded_cluster_detail_pane(detail: ClusterDetail) -> ClusterDetailPane {
    let mut pane = ClusterDetailPane {
        detail: Loadable::loaded(Some(detail)),
        ..ClusterDetailPane::default()
    };
    pane.loaded();
    pane
}

pub fn sample_run_record() -> forgesync_store::runs::RunRecord {
    let timestamp = UtcTimestamp::parse("2026-09-29T00:00:00Z").expect("timestamp");
    forgesync_store::runs::RunRecord {
        id: forgesync_core::identity::RunId::new(23).expect("run identity"),
        parent_id: None,
        status: forgesync_store::runs::RunStatus::Failed,
        started_at: timestamp,
        updated_at: timestamp,
        finished_at: Some(timestamp),
        scope: Default::default(),
        outcome: None,
    }
}
