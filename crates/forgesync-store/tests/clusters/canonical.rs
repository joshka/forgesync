//! # Canonical member validation
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

use forgesync_core::document::DocumentRecipe;
use forgesync_core::observation::ThreadObservation;
use forgesync_store::archive::Archive;
use forgesync_store::clusters::ClusterGenerationInput;

use crate::fixture::{
    active_clusters, cluster, discussion, remove_archive, repository, temporary_archive_path,
    thread_id, timestamp,
};

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
    let observation = ThreadObservation {
        discussion: discussion(&first),
        observed_at,
        sequence,
    };
    archive
        .apply_thread_observation(&observation, None)
        .await
        .expect("apply thread observation");
    let observed_at = timestamp("2026-09-20T10:00:00Z");
    let sequence = archive
        .reserve_observation_sequence(observed_at)
        .await
        .expect("reserve thread sequence");
    let observation = ThreadObservation {
        discussion: discussion(&second),
        observed_at,
        sequence,
    };
    archive
        .apply_thread_observation(&observation, None)
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
