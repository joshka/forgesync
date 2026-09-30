//! # Cluster persistence integration
//!
//! These cases store generations, memberships, and maintainer decisions, then inspect their
//! projections. They separate derived cluster proposals from local human actions. Engine tests own
//! candidate scoring; this suite protects durable representation and query behavior.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use forgesync_store::clusters::{
    ClusterGenerationInput, ClusterInput, ClusterLifecycle, ClusterListQuery, ClusterMemberInput,
    ClusterMemberRole, ClusterMemberState,
};

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[tokio::test]
async fn partial_generations_preserve_groups_and_complete_generations_retire_them() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository();
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let first = thread_id(&repository.id, "thread-1", 1);
    let second = thread_id(&repository.id, "thread-2", 2);
    let third = thread_id(&repository.id, "thread-3", 3);
    let observed_at = timestamp("2026-09-20T10:00:00Z");
    let sequence = archive
        .reserve_observation_sequence(observed_at)
        .await
        .expect("reserve thread sequence");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        discussion(&first),
        SourceClock::Valid(observed_at),
        observed_at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply thread observation");
    let observed_at = timestamp("2026-09-20T10:00:00Z");
    let sequence = archive
        .reserve_observation_sequence(observed_at)
        .await
        .expect("reserve thread sequence");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        discussion(&second),
        SourceClock::Valid(observed_at),
        observed_at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply thread observation");
    let observed_at = timestamp("2026-09-20T10:00:00Z");
    let sequence = archive
        .reserve_observation_sequence(observed_at)
        .await
        .expect("reserve thread sequence");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        discussion(&third),
        SourceClock::Valid(observed_at),
        observed_at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply thread observation");
    let at = timestamp("2035-01-01T00:00:00Z");
    let lease = archive
        .acquire_archive_lease(at, Duration::from_secs(3600))
        .await
        .expect("acquire writer fence");

    let first_run = archive
        .save_clusters_fenced(
            &lease,
            &ClusterGenerationInput {
                repository: repository.id.clone(),
                endpoint: "https://embeddings.example/v1".to_owned(),
                model: "model-v1".to_owned(),
                recipe: DocumentRecipe::OriginalBody,
                complete_coverage: true,
                eligible_threads: 3,
                vector_threads: 3,
                candidate_edges: 1,
                clusters: vec![
                    cluster("first related", &first, &[(&first, 1.0), (&second, 0.82)]),
                    cluster("third", &third, &[(&third, 1.0)]),
                ],
            },
            at,
        )
        .await
        .expect("save complete generation");
    assert_eq!(first_run.cluster_count, 2);
    assert_eq!(first_run.member_count, 3);
    assert_eq!(first_run.retired_count, 0);

    let first_page = archive
        .list_clusters(&all_clusters(&repository.id))
        .await
        .expect("list initial clusters");
    let first_cluster = first_page
        .items
        .iter()
        .find(|cluster| cluster.title == "first related")
        .expect("first cluster")
        .id;
    let third_cluster = first_page
        .items
        .iter()
        .find(|cluster| cluster.title == "third")
        .expect("third cluster")
        .id;

    archive
        .dismiss_cluster_fenced(&lease, first_cluster, "known duplicate", at)
        .await
        .expect("dismiss cluster");
    archive
        .exclude_cluster_member_fenced(&lease, first_cluster, &second, "different root cause", at)
        .await
        .expect("exclude member");
    archive
        .set_cluster_canonical_fenced(&lease, first_cluster, &first, at)
        .await
        .expect("select canonical member");

    let partial = archive
        .save_clusters_fenced(
            &lease,
            &ClusterGenerationInput {
                repository: repository.id.clone(),
                endpoint: "https://embeddings.example/v1".to_owned(),
                model: "model-v1".to_owned(),
                recipe: DocumentRecipe::OriginalBody,
                complete_coverage: false,
                eligible_threads: 3,
                vector_threads: 2,
                candidate_edges: 1,
                clusters: vec![cluster(
                    "first refreshed",
                    &first,
                    &[(&first, 1.0), (&second, 0.82)],
                )],
            },
            at,
        )
        .await
        .expect("save partial generation");
    assert_eq!(partial.retired_count, 0);
    let after_partial = archive
        .list_clusters(&all_clusters(&repository.id))
        .await
        .expect("list after partial generation");
    assert!(after_partial.items.iter().any(|cluster| {
        cluster.id == third_cluster && cluster.lifecycle == ClusterLifecycle::Active
    }));
    let refreshed = after_partial
        .items
        .iter()
        .find(|cluster| cluster.id == first_cluster)
        .expect("matched cluster identity");
    assert!(refreshed.dismissed);
    assert_eq!(
        refreshed.dismissal_reason.as_deref(),
        Some("known duplicate")
    );
    assert_eq!(refreshed.title, "first refreshed");
    assert_eq!(
        refreshed
            .representative
            .as_ref()
            .map(|value| value.number().get()),
        Some(1)
    );
    assert_eq!(refreshed.active_member_count, 1);
    assert_eq!(refreshed.excluded_member_count, 1);

    let detail = archive
        .cluster_detail(first_cluster)
        .await
        .expect("read cluster detail");
    let canonical = detail
        .members
        .iter()
        .find(|member| member.summary.discussion.id == first)
        .expect("canonical member");
    assert_eq!(canonical.role, ClusterMemberRole::Canonical);
    assert_eq!(canonical.state, ClusterMemberState::Active);
    let excluded = detail
        .members
        .iter()
        .find(|member| member.summary.discussion.id == second)
        .expect("excluded member");
    assert_eq!(excluded.state, ClusterMemberState::Excluded);
    assert_eq!(excluded.score_to_representative, Some(0.82));

    let complete = archive
        .save_clusters_fenced(
            &lease,
            &ClusterGenerationInput {
                repository: repository.id.clone(),
                endpoint: "https://embeddings.example/v1".to_owned(),
                model: "model-v1".to_owned(),
                recipe: DocumentRecipe::OriginalBody,
                complete_coverage: true,
                eligible_threads: 3,
                vector_threads: 3,
                candidate_edges: 1,
                clusters: vec![cluster("first only", &first, &[(&first, 1.0)])],
            },
            at,
        )
        .await
        .expect("save complete generation");
    assert_eq!(complete.retired_count, 1);
    let active = archive
        .list_clusters(&active_clusters(&repository.id))
        .await
        .expect("list active clusters");
    assert_eq!(active.items.len(), 1);
    assert_eq!(active.items[0].id, first_cluster);
    assert_eq!(active.items[0].excluded_member_count, 0);
    let all = archive
        .list_clusters(&all_clusters(&repository.id))
        .await
        .expect("list including retired clusters");
    assert_eq!(all.items.len(), 2);
    assert!(all.items.iter().any(|cluster| {
        cluster.id == third_cluster && cluster.lifecycle == ClusterLifecycle::Retired
    }));

    archive
        .release_archive_lease(&lease, at)
        .await
        .expect("release writer fence");
    archive.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn canonical_selection_rejects_a_thread_outside_the_cluster() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository();
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let first = thread_id(&repository.id, "thread-1", 1);
    let second = thread_id(&repository.id, "thread-2", 2);
    let observed_at = timestamp("2026-09-20T10:00:00Z");
    let sequence = archive
        .reserve_observation_sequence(observed_at)
        .await
        .expect("reserve thread sequence");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        discussion(&first),
        SourceClock::Valid(observed_at),
        observed_at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply thread observation");
    let observed_at = timestamp("2026-09-20T10:00:00Z");
    let sequence = archive
        .reserve_observation_sequence(observed_at)
        .await
        .expect("reserve thread sequence");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        discussion(&second),
        SourceClock::Valid(observed_at),
        observed_at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply thread observation");
    let at = timestamp("2035-01-01T00:00:00Z");
    let lease = archive
        .acquire_archive_lease(at, Duration::from_secs(3600))
        .await
        .expect("acquire writer fence");

    archive
        .save_clusters_fenced(
            &lease,
            &ClusterGenerationInput {
                repository: repository.id.clone(),
                endpoint: "https://embeddings.example/v1".to_owned(),
                model: "model-v1".to_owned(),
                recipe: DocumentRecipe::OriginalBody,
                complete_coverage: true,
                eligible_threads: 1,
                vector_threads: 1,
                candidate_edges: 1,
                clusters: vec![cluster("only", &first, &[(&first, 1.0)])],
            },
            at,
        )
        .await
        .expect("save valid generation");
    let cluster_id = archive
        .list_clusters(&active_clusters(&repository.id))
        .await
        .expect("list valid generation")
        .items[0]
        .id;
    let error = archive
        .set_cluster_canonical_fenced(&lease, cluster_id, &second, at)
        .await
        .expect_err("canonical choice must be a cluster member");
    assert!(matches!(
        error,
        forgesync_store::error::StoreError::ClusterMemberMissing
    ));

    archive
        .release_archive_lease(&lease, at)
        .await
        .expect("release writer fence");
    archive.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn complete_generation_rejects_missing_vector_coverage_without_storing_clusters() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository();
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let first = thread_id(&repository.id, "thread-1", 1);
    let observed_at = timestamp("2026-09-20T10:00:00Z");
    let sequence = archive
        .reserve_observation_sequence(observed_at)
        .await
        .expect("reserve thread sequence");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        discussion(&first),
        SourceClock::Valid(observed_at),
        observed_at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply thread observation");
    let at = timestamp("2035-01-01T00:00:00Z");
    let lease = archive
        .acquire_archive_lease(at, Duration::from_secs(3600))
        .await
        .expect("acquire writer fence");
    let invalid = ClusterGenerationInput {
        repository: repository.id.clone(),
        endpoint: "https://embeddings.example/v1".to_owned(),
        model: "model-v1".to_owned(),
        recipe: DocumentRecipe::OriginalBody,
        complete_coverage: true,
        eligible_threads: 1,
        vector_threads: 0,
        candidate_edges: 0,
        clusters: vec![cluster("invalid", &first, &[(&first, 1.0)])],
    };

    let error = archive
        .save_clusters_fenced(&lease, &invalid, at)
        .await
        .expect_err("complete coverage requires vectors for all eligible threads");

    assert!(matches!(
        error,
        forgesync_store::error::StoreError::InvalidClusterGeneration
    ));
    let page = archive
        .list_clusters(&all_clusters(&repository.id))
        .await
        .expect("list rejected generation");
    assert!(page.items.is_empty());
    archive
        .release_archive_lease(&lease, at)
        .await
        .expect("release writer fence");
    archive.close().await;
    remove_archive(&path);
}

/// Constructs proposed membership from supplied identities and scores without running analysis.
fn cluster(title: &str, representative: &ThreadId, members: &[(&ThreadId, f64)]) -> ClusterInput {
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
fn active_clusters(repository: &RepositoryId) -> ClusterListQuery<'_> {
    ClusterListQuery {
        repositories: std::slice::from_ref(repository),
        include_retired: false,
        limit: NonZeroU32::new(100).expect("positive limit"),
        offset: 0,
    }
}

/// Includes retired generations when a scenario inspects historical cluster lifecycle.
fn all_clusters(repository: &RepositoryId) -> ClusterListQuery<'_> {
    ClusterListQuery {
        include_retired: true,
        ..active_clusters(repository)
    }
}

/// Constructs one fixed open issue without reserving a sequence or writing the archive.
fn discussion(thread: &ThreadId) -> Discussion {
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
fn repository() -> Repository {
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
fn thread_id(repository: &RepositoryId, provider_id: &str, number: u64) -> ThreadId {
    ThreadId::new(
        repository.clone(),
        ProviderId::new(provider_id).expect("thread provider ID"),
        ThreadNumber::new(number).expect("thread number"),
    )
}

/// Parses a fixed scenario timestamp without sampling the process clock.
fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
}

/// Allocates a process-local unique filename without creating or opening an archive.
fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-clusters-{}-{sequence}.sqlite",
        std::process::id()
    ))
}

/// Removes the closed database and its possible WAL sidecars on a best-effort basis.
///
/// The fixed suffix loop is cleanup only; it does not select scenarios or compute expectations.
fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}
