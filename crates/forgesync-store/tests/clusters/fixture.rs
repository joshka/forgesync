//! Cluster scenario value builders; shared path and identity helpers live in `common`.

use std::num::NonZeroU32;

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::identity::{RepositoryId, ThreadId};
use forgesync_core::provider_data::ProviderData;
use forgesync_store::clusters::{ClusterInput, ClusterListQuery, ClusterMemberInput};

pub use crate::common::{remove_archive, temporary_archive_path, thread_id, timestamp};

/// Constructs proposed membership from supplied identities and scores without running analysis.
pub fn cluster(
    title: &str,
    representative: &ThreadId,
    members: &[(&ThreadId, f64)],
) -> ClusterInput {
    ClusterInput {
        representative: representative.clone(),
        title: title.to_owned(),
        members: members
            .iter()
            .map(|(thread, score)| ClusterMemberInput {
                thread: (*thread).clone(),
                score_to_representative: Some(*score),
            })
            .collect(),
    }
}

/// Selects current clusters for one repository without retired generations.
pub fn active_clusters(repository: &RepositoryId) -> ClusterListQuery<'_> {
    ClusterListQuery {
        repositories: std::slice::from_ref(repository),
        include_retired: false,
        limit: NonZeroU32::new(100).expect("positive limit"),
        offset: 0,
    }
}

/// Includes retired generations when a scenario inspects historical cluster lifecycle.
pub fn all_clusters(repository: &RepositoryId) -> ClusterListQuery<'_> {
    ClusterListQuery {
        include_retired: true,
        ..active_clusters(repository)
    }
}

/// Constructs one fixed open issue without reserving a sequence or writing the archive.
pub fn discussion(thread: &ThreadId) -> Discussion {
    Discussion {
        id: thread.clone(),
        kind: ThreadKind::Issue,
        state: SourceState::Open,
        title: format!("Thread {}", thread.number().get()),
        body: Some(format!("Body for thread {}", thread.number().get())),
        html_url: None,
        created_at: timestamp("2026-09-19T10:00:00Z"),
        updated_at: timestamp("2026-09-20T10:00:00Z"),
        closed_at: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        provider_data: ProviderData::new(),
    }
}

/// Constructs fixed repository metadata without registering it in the archive.
pub fn repository() -> Repository {
    crate::common::repository("example", "clusters", "repo-cluster-tests")
}
