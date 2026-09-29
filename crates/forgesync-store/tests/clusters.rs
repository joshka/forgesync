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
use forgesync_store::{
    Archive, ClusterGenerationInput, ClusterInput, ClusterLifecycle, ClusterListQuery,
    ClusterMemberInput, ClusterMemberRole, ClusterMemberState,
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
    for (thread, number) in [(&first, 1), (&second, 2), (&third, 3)] {
        apply_thread(&archive, thread, number).await;
    }
    let at = timestamp("2035-01-01T00:00:00Z");
    let lease = archive
        .acquire_archive_lease(at, Duration::from_secs(3600))
        .await
        .expect("acquire writer fence");

    let first_run = archive
        .save_clusters_fenced(
            &lease,
            &generation(
                &repository.id,
                true,
                3,
                3,
                vec![
                    cluster("first related", &first, &[(&first, 1.0), (&second, 0.82)]),
                    cluster("third", &third, &[(&third, 1.0)]),
                ],
            ),
            at,
        )
        .await
        .expect("save complete generation");
    assert_eq!(first_run.cluster_count, 2);
    assert_eq!(first_run.member_count, 3);
    assert_eq!(first_run.retired_count, 0);

    let first_page = archive
        .list_clusters(&list_query(&repository.id, true))
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
        .set_cluster_dismissed_fenced(&lease, first_cluster, true, "known duplicate", at)
        .await
        .expect("dismiss cluster");
    archive
        .set_cluster_member_excluded_fenced(
            &lease,
            first_cluster,
            &second,
            true,
            "different root cause",
            at,
        )
        .await
        .expect("exclude member");
    archive
        .set_cluster_canonical_fenced(&lease, first_cluster, &first, at)
        .await
        .expect("select canonical member");

    let partial = archive
        .save_clusters_fenced(
            &lease,
            &generation(
                &repository.id,
                false,
                3,
                2,
                vec![cluster(
                    "first refreshed",
                    &first,
                    &[(&first, 1.0), (&second, 0.82)],
                )],
            ),
            at,
        )
        .await
        .expect("save partial generation");
    assert_eq!(partial.retired_count, 0);
    let after_partial = archive
        .list_clusters(&list_query(&repository.id, true))
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
            &generation(
                &repository.id,
                true,
                3,
                3,
                vec![cluster("first only", &first, &[(&first, 1.0)])],
            ),
            at,
        )
        .await
        .expect("save complete generation");
    assert_eq!(complete.retired_count, 1);
    let active = archive
        .list_clusters(&list_query(&repository.id, false))
        .await
        .expect("list active clusters");
    assert_eq!(active.items.len(), 1);
    assert_eq!(active.items[0].id, first_cluster);
    assert_eq!(active.items[0].excluded_member_count, 0);
    let all = archive
        .list_clusters(&list_query(&repository.id, true))
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
async fn invalid_member_decisions_and_incomplete_counts_are_rejected() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository();
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let first = thread_id(&repository.id, "thread-1", 1);
    let second = thread_id(&repository.id, "thread-2", 2);
    apply_thread(&archive, &first, 1).await;
    apply_thread(&archive, &second, 2).await;
    let at = timestamp("2035-01-01T00:00:00Z");
    let lease = archive
        .acquire_archive_lease(at, Duration::from_secs(3600))
        .await
        .expect("acquire writer fence");

    let invalid = generation(
        &repository.id,
        true,
        1,
        0,
        vec![cluster("invalid", &first, &[(&first, 1.0)])],
    );
    assert!(
        archive
            .save_clusters_fenced(&lease, &invalid, at)
            .await
            .is_err()
    );

    archive
        .save_clusters_fenced(
            &lease,
            &generation(
                &repository.id,
                true,
                1,
                1,
                vec![cluster("only", &first, &[(&first, 1.0)])],
            ),
            at,
        )
        .await
        .expect("save valid generation");
    let cluster_id = archive
        .list_clusters(&list_query(&repository.id, false))
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
        forgesync_store::StoreError::ClusterMemberMissing
    ));

    archive
        .release_archive_lease(&lease, at)
        .await
        .expect("release writer fence");
    archive.close().await;
    remove_archive(&path);
}

fn generation(
    repository: &RepositoryId,
    complete_coverage: bool,
    eligible_threads: u64,
    vector_threads: u64,
    clusters: Vec<ClusterInput>,
) -> ClusterGenerationInput {
    ClusterGenerationInput {
        repository: repository.clone(),
        endpoint: "https://embeddings.example/v1".to_owned(),
        model: "model-v1".to_owned(),
        recipe: DocumentRecipe::OriginalBody,
        complete_coverage,
        eligible_threads,
        vector_threads,
        candidate_edges: 1,
        clusters,
    }
}

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

fn list_query<'a>(repository: &'a RepositoryId, include_retired: bool) -> ClusterListQuery<'a> {
    ClusterListQuery {
        repositories: std::slice::from_ref(repository),
        include_retired,
        limit: NonZeroU32::new(100).expect("positive limit"),
        offset: 0,
    }
}

async fn apply_thread(archive: &Archive, thread: &ThreadId, number: u64) {
    let updated_at = timestamp("2026-09-20T10:00:00Z");
    let sequence = archive
        .reserve_observation_sequence(updated_at)
        .await
        .expect("reserve observation sequence");
    let discussion = Discussion {
        id: thread.clone(),
        kind: ThreadKind::Issue,
        state: SourceState::Open,
        title: format!("Thread {number}"),
        body: Some(format!("Body for thread {number}")),
        html_url: None,
        created_at: timestamp("2026-09-19T10:00:00Z"),
        updated_at,
        closed_at: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        provider_data: ProviderData::new(),
    };
    let raw_clock = updated_at.format_rfc3339().expect("format source clock");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        discussion,
        SourceClock::from_raw(Some(&raw_clock)),
        updated_at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply thread observation");
}

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

fn thread_id(repository: &RepositoryId, provider_id: &str, number: u64) -> ThreadId {
    ThreadId::new(
        repository.clone(),
        ProviderId::new(provider_id).expect("thread provider ID"),
        ThreadNumber::new(number).expect("thread number"),
    )
}

fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
}

fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-clusters-{}-{sequence}.sqlite",
        std::process::id()
    ))
}

fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}
