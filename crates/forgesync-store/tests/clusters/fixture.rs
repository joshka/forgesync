//! # Cluster persistence construction and cleanup fixtures
//!
//! These helpers construct fixed normalized records, proposed membership, and read query values.
//! Membership scores and representatives remain supplied by each scenario, with no graph analysis.
//! Discussion titles derive from checked display numbers; repository and source timestamps are
//! fixed. No helper reserves an observation sequence, applies content, or saves a cluster
//! generation.
//!
//! Filename allocation and closed-database cleanup are the only filesystem effects.
//! The fixed sidecar loop is resource cleanup, not scenario selection or assertion calculation.
//! Scenario files own all archive transitions and expected outcomes with direct imports here.
//! Keep incidental valid identity construction here and policy-sensitive generation counts there.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::clusters::{ClusterInput, ClusterListQuery, ClusterMemberInput};

/// Separates archive filenames for concurrent cases within this process.
static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

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
    let id = RepositoryId::new(
        GitHubHost::parse("github.com").expect("host"),
        ProviderId::new("repo-cluster-tests").expect("repository provider ID"),
    );
    Repository {
        id,
        owner: "example".to_owned(),
        name: "clusters".to_owned(),
        full_name: "example/clusters".to_owned(),
        default_branch: Some("main".to_owned()),
        updated_at: Some(timestamp("2026-09-20T10:00:00Z")),
        provider_data: ProviderData::new(),
    }
}

/// Checks explicit repository, provider identity, and display number for a fixture discussion.
pub fn thread_id(repository: &RepositoryId, provider_id: &str, number: u64) -> ThreadId {
    ThreadId::new(
        repository.clone(),
        ProviderId::new(provider_id).expect("thread provider ID"),
        ThreadNumber::new(number).expect("thread number"),
    )
}

/// Parses a fixed scenario timestamp without sampling the process clock.
pub fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
}

/// Allocates a process-local unique filename without creating or opening an archive.
pub fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-clusters-{}-{sequence}.sqlite",
        std::process::id()
    ))
}

/// Removes the closed database and its possible WAL sidecars on a best-effort basis.
///
/// The fixed suffix loop is cleanup only; it does not select scenarios or compute expectations.
pub fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}
