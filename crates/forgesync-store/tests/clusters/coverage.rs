//! # Complete vector coverage validation
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
use forgesync_store::clusters::ClusterGenerationInput;

use crate::fixture::{
    all_clusters, cluster, discussion, remove_archive, repository, temporary_archive_path,
    thread_id, timestamp,
};

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
        .apply_thread_observation(&observation, None)
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
