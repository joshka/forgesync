//! # Partial vector coverage preserves the prior cluster
//!
//! This dependent transition first publishes two current vectors and establishes a complete
//! cluster. A newer parent observation for one member then invalidates its document's source
//! coordinates. The next build sees two eligible discussions but only one current vector.
//!
//! The partial generation must retain the previous group identity, active lifecycle, and two-member
//! membership instead of retiring a group that cannot be reconstructed from incomplete evidence.
//! Earlier assertions remain in this scenario because they prove that the preserved group existed.
//!
//! Observation, fenced document/vector writes, build, and list calls remain explicit. Fixtures only
//! construct payloads. Complete creation and unmatched vector namespaces have independent
//! scenarios.

use std::time::Duration;

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::document::DocumentRecipe;
use forgesync_core::embedding::EmbeddingVector;
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_engine::clustering::{
    ClusterBuildRequest, ClusterListRequest, ClusterOptions, build_clusters, list_clusters,
};
use forgesync_engine::reference::RepositorySelector;
use forgesync_store::archive::Archive;
use forgesync_store::clusters::ClusterLifecycle;
use forgesync_store::embeddings::EmbeddingChunkInput;
use tokio_util::sync::CancellationToken;

use crate::fixture::{
    discussion, document, remove_archive, repository, temporary_archive_path, thread_id, timestamp,
};

#[tokio::test]
async fn partial_vector_coverage_preserves_the_previous_cluster() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository();
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let first = thread_id(&repository.id, "thread-1", 1);
    let second = thread_id(&repository.id, "thread-2", 2);
    let first_updated = timestamp("2026-09-20T10:00:00Z");
    let second_updated = timestamp("2026-09-20T10:00:01Z");
    let content = discussion(&first, first_updated);
    let sequence = archive
        .reserve_observation_sequence(first_updated)
        .await
        .expect("reserve first observation sequence");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        content,
        SourceClock::Valid(first_updated),
        first_updated,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation, None)
        .await
        .expect("apply first observation");
    let content = discussion(&second, second_updated);
    let sequence = archive
        .reserve_observation_sequence(second_updated)
        .await
        .expect("reserve second observation sequence");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        content,
        SourceClock::Valid(second_updated),
        second_updated,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation, None)
        .await
        .expect("apply second observation");
    let endpoint = "https://embeddings.example/v1";
    let model = "test-model";
    let lease_at = timestamp("2035-01-01T00:00:00Z");
    let lease = archive
        .acquire_archive_lease(lease_at, Duration::from_secs(3600))
        .await
        .expect("acquire archive fence");
    let first_document = document(&first, first_updated);
    let second_document = document(&second, second_updated);
    let acquired_at = timestamp("2035-01-01T00:00:01Z");
    let vector = EmbeddingVector::new(vec![1.0, 0.0], None).expect("valid fixture vector");
    archive
        .upsert_document_fenced(&lease, &first_document, acquired_at)
        .await
        .expect("store first document");
    let first_chunk = EmbeddingChunkInput {
        endpoint,
        model,
        index: 0,
        count: 1,
        chunk_hash: &first_document.content_hash,
        vector: &vector,
    };
    archive
        .upsert_embedding_chunk_fenced(&lease, &first_document, &first_chunk, acquired_at)
        .await
        .expect("store first vector");
    archive
        .upsert_document_fenced(&lease, &second_document, acquired_at)
        .await
        .expect("store second document");
    let second_chunk = EmbeddingChunkInput {
        endpoint,
        model,
        index: 0,
        count: 1,
        chunk_hash: &second_document.content_hash,
        vector: &vector,
    };
    archive
        .upsert_embedding_chunk_fenced(&lease, &second_document, &second_chunk, acquired_at)
        .await
        .expect("store second vector");
    archive
        .release_archive_lease(&lease, lease_at)
        .await
        .expect("release archive fence");

    let request = ClusterBuildRequest {
        repository: RepositorySelector::from_repository(&repository),
        endpoint: endpoint.to_owned(),
        model: model.to_owned(),
        recipe: DocumentRecipe::OriginalBody,
        options: ClusterOptions::default(),
    };
    let cancellation = CancellationToken::new();
    let complete = build_clusters(&archive, &request, &cancellation)
        .await
        .expect("build complete clusters");
    assert!(complete.generation.complete_coverage);
    assert_eq!(complete.eligible_threads, 2);
    assert_eq!(complete.vector_threads, 2);
    assert_eq!(complete.generation.cluster_count, 1);
    assert_eq!(complete.generation.member_count, 2);

    let listed = list_clusters(
        &archive,
        &ClusterListRequest {
            repositories: vec![RepositorySelector::from_repository(&repository)],
            include_retired: false,
            limit: 100,
            offset: 0,
        },
    )
    .await
    .expect("list generated cluster");
    assert_eq!(listed.items.len(), 1);
    let cluster_id = listed.items[0].id;

    let newer_update = timestamp("2026-09-20T10:00:02Z");
    let content = discussion(&second, newer_update);
    let sequence = archive
        .reserve_observation_sequence(newer_update)
        .await
        .expect("reserve second observation sequence");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        content,
        SourceClock::Valid(newer_update),
        newer_update,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation, None)
        .await
        .expect("apply second observation");

    let partial = build_clusters(&archive, &request, &cancellation)
        .await
        .expect("build partial clusters from remaining current vectors");
    assert!(!partial.generation.complete_coverage);
    assert_eq!(partial.eligible_threads, 2);
    assert_eq!(partial.vector_threads, 1);
    assert_eq!(partial.generation.retired_count, 0);
    let after_partial = list_clusters(
        &archive,
        &ClusterListRequest {
            repositories: vec![RepositorySelector::from_repository(&repository)],
            include_retired: true,
            limit: 100,
            offset: 0,
        },
    )
    .await
    .expect("list after partial generation");
    assert_eq!(after_partial.items.len(), 1);
    assert_eq!(after_partial.items[0].id, cluster_id);
    assert_eq!(after_partial.items[0].lifecycle, ClusterLifecycle::Active);
    assert_eq!(after_partial.items[0].active_member_count, 2);

    archive.close().await;
    remove_archive(&path);
}
