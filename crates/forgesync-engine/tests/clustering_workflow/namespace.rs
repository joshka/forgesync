//! # Vector namespaces cannot be substituted during clustering
//!
//! The archive contains two current vectors under the configured fixture endpoint and model. A
//! build requesting a different endpoint must reject unavailable vectors rather than reuse those
//! embeddings or treat the repository as a valid empty candidate set.
//!
//! Archive creation, evidence application, fenced document/vector persistence, and the actual build
//! are visible next to the exact error expectation. Namespace rejection requires no previous
//! cluster generation; that unrelated operation stays in the complete and partial scenarios.
//!
//! Pure construction fixtures supply matching original-body documents. No embedding HTTP service is
//! involved; this regression checks engine selection against persisted representation identity.

use std::time::Duration;

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::document::DocumentRecipe;
use forgesync_core::embedding::EmbeddingVector;
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_engine::clustering::{ClusterBuildRequest, ClusterOptions, build_clusters};
use forgesync_engine::error::EngineError;
use forgesync_engine::reference::RepositorySelector;
use forgesync_store::archive::Archive;
use forgesync_store::embeddings::EmbeddingChunkInput;
use tokio_util::sync::CancellationToken;

use crate::fixture::{
    discussion, document, remove_archive, repository, temporary_archive_path, thread_id, timestamp,
};

#[tokio::test]
async fn unmatched_endpoint_rejects_otherwise_current_vectors() {
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
    let no_vectors = ClusterBuildRequest {
        endpoint: "https://unconfigured.example/v1".to_owned(),
        ..request
    };
    let result = build_clusters(&archive, &no_vectors, &cancellation).await;
    assert!(matches!(
        result,
        Err(EngineError::ClusterVectorsUnavailable)
    ));
    archive.close().await;
    remove_archive(&path);
}
