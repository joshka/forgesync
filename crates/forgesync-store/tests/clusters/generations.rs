//! # Local decision retention across generation replacement
//!
//! This scenario performs sequence reservation, observation writes, generation persistence,
//! and local inspection directly through the public archive API.
//! Construction fixtures supply normalized content and proposed membership without archive writes.
//! Generation inputs name completeness and counts at the point where they are saved.
//!
//! The archive is on disk; the scenario owns writer acquisition, release, closure, and cleanup.
//! A fixed future acquisition time keeps fencing eligible while avoiding process-clock fixtures.
//! Engine suites own vector scoring and candidate selection; this suite supplies their result.
//! Assertions protect durable membership and local policy rather than the graph algorithm.

use std::time::Duration;

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::document::DocumentRecipe;
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_store::archive::Archive;
use forgesync_store::clusters::{ClusterGenerationInput, ClusterMemberRole, ClusterMemberState};

use crate::fixture::{
    all_clusters, cluster, discussion, remove_archive, repository, temporary_archive_path,
    thread_id, timestamp,
};

#[tokio::test]
async fn replacement_preserves_dismissal_exclusion_and_canonical_selection() {
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
        .apply_thread_observation(&observation, None)
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
        .apply_thread_observation(&observation, None)
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
                eligible_threads: 2,
                vector_threads: 2,
                candidate_edges: 1,
                clusters: vec![cluster(
                    "first related",
                    &first,
                    &[(&first, 1.0), (&second, 0.82)],
                )],
            },
            at,
        )
        .await
        .expect("save complete generation");
    assert_eq!(first_run.cluster_count, 1);
    assert_eq!(first_run.member_count, 2);
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
                eligible_threads: 2,
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

    archive
        .release_archive_lease(&lease, at)
        .await
        .expect("release writer fence");
    archive.close().await;
    remove_archive(&path);
}
