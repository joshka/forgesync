//! # Replacement coverage and unmatched durable groups
//!
//! Two existing singleton groups establish matched and omitted membership explicitly.
//! The replacement proposes only the first group and names its coverage and vector counts.
//! The omitted group's lifecycle distinguishes preservation from retirement.
//! Each test reserves observations and saves both generations directly, without setup workflows.
//!
//! Construction fixtures provide values and queries only; no helper runs archive operations.
//! Fixed future lease time keeps transaction fencing eligible throughout the local scenario.
//! Candidate scoring, human decisions, and identity tie-breaking have separate suites.
//! Cleanup follows explicit release and closure of the on-disk archive.

use std::time::Duration;

use forgesync_core::document::DocumentRecipe;
use forgesync_core::observation::ThreadObservation;
use forgesync_store::archive::Archive;
use forgesync_store::clusters::{ClusterGenerationInput, ClusterLifecycle};

use crate::fixture::{
    all_clusters, cluster, discussion, remove_archive, repository, temporary_archive_path,
    thread_id, timestamp,
};

#[tokio::test]
async fn complete_replacement_retires_an_omitted_group() {
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

    let initial = ClusterGenerationInput {
        repository: repository.id.clone(),
        endpoint: "https://embeddings.example/v1".to_owned(),
        model: "model-v1".to_owned(),
        recipe: DocumentRecipe::OriginalBody,
        complete_coverage: true,
        eligible_threads: 2,
        vector_threads: 2,
        candidate_edges: 0,
        clusters: vec![
            cluster("first", &first, &[(&first, 1.0)]),
            cluster("second", &second, &[(&second, 1.0)]),
        ],
    };
    let initial_result = archive
        .save_clusters_fenced(&lease, &initial, at)
        .await
        .expect("save initial generation");
    assert_eq!(initial_result.cluster_count, 2);
    let before = archive
        .list_clusters(&all_clusters(&repository.id))
        .await
        .expect("initial groups");
    let omitted_id = before
        .items
        .iter()
        .find(|group| group.title == "second")
        .expect("omitted group")
        .id;
    let replacement = ClusterGenerationInput {
        repository: repository.id.clone(),
        endpoint: "https://embeddings.example/v1".to_owned(),
        model: "model-v1".to_owned(),
        recipe: DocumentRecipe::OriginalBody,
        complete_coverage: true,
        eligible_threads: 2,
        vector_threads: 2,
        candidate_edges: 0,
        clusters: vec![cluster("first refreshed", &first, &[(&first, 1.0)])],
    };
    let replacement_result = archive
        .save_clusters_fenced(&lease, &replacement, at)
        .await
        .expect("save replacement generation");
    assert_eq!(replacement_result.retired_count, 1);
    let after = archive
        .list_clusters(&all_clusters(&repository.id))
        .await
        .expect("replacement groups");
    assert_eq!(after.items.len(), 2);
    let omitted = after
        .items
        .iter()
        .find(|group| group.id == omitted_id)
        .expect("original group identity retained");
    assert_eq!(omitted.lifecycle, ClusterLifecycle::Retired);
    assert_eq!(omitted.title, "second");
    archive
        .release_archive_lease(&lease, at)
        .await
        .expect("release writer fence");
    archive.close().await;
    remove_archive(&path);
}
