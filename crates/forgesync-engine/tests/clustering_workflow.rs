//! # Cluster workflow integration
//!
//! These cases build and inspect cluster generations against a real local archive. They cover the
//! path from candidate analysis through durable membership and local decisions. Pure candidate
//! rules live beside `clustering/candidates`; this suite checks the boundaries between engine and
//! store.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::embedding::EmbeddingVector;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_engine::clustering::{
    ClusterBuildRequest, ClusterListRequest, ClusterOptions, build_clusters, list_clusters,
};
use forgesync_engine::error::EngineError;
use forgesync_engine::reference::RepositorySelector;
use forgesync_store::archive::Archive;
use forgesync_store::clusters::ClusterLifecycle;
use forgesync_store::embeddings::EmbeddingChunkInput;
use tokio_util::sync::CancellationToken;

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[tokio::test]
async fn build_uses_current_open_vectors_and_only_retires_with_complete_coverage() {
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
    apply_thread(&archive, &first, first_updated).await;
    apply_thread(&archive, &second, second_updated).await;
    let endpoint = "https://embeddings.example/v1";
    let model = "test-model";
    let lease_at = timestamp("2035-01-01T00:00:00Z");
    let lease = archive
        .acquire_archive_lease(lease_at, Duration::from_secs(3600))
        .await
        .expect("acquire archive fence");
    let first_document = document(&first, first_updated);
    let second_document = document(&second, second_updated);
    save_document_vector(
        &archive,
        &lease,
        &first_document,
        endpoint,
        model,
        &[1.0, 0.0],
    )
    .await;
    save_document_vector(
        &archive,
        &lease,
        &second_document,
        endpoint,
        model,
        &[1.0, 0.0],
    )
    .await;
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

    apply_thread(&archive, &second, timestamp("2026-09-20T10:00:02Z")).await;
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

    let no_vectors = ClusterBuildRequest {
        endpoint: "https://unconfigured.example/v1".to_owned(),
        ..request
    };
    assert!(matches!(
        build_clusters(&archive, &no_vectors, &cancellation).await,
        Err(EngineError::ClusterVectorsUnavailable)
    ));
    archive.close().await;
    remove_archive(&path);
}

/// Builds the original-body recipe matching the synthetic issue, without persisting it.
fn document(thread: &ThreadId, updated_at: UtcTimestamp) -> Document {
    let title = format!("Shared cache failure {}", thread.number().get());
    let text = format!("{title}\n\nThe cache fails after restart.");
    Document::new(
        thread.clone(),
        DocumentRecipe::OriginalBody,
        title,
        text.clone(),
        text.to_ascii_lowercase(),
        updated_at,
    )
}

/// Stores the current document and one explicit vector chunk under the caller-owned fence.
///
/// The scenario controls endpoint, model, and values independently; no embedding service is called.
/// Writes use a fixed acquisition time inside the fixture lease, with the document hash as chunk
/// hash.
async fn save_document_vector(
    archive: &Archive,
    lease: &forgesync_store::leases::ArchiveLeaseToken,
    document: &Document,
    endpoint: &str,
    model: &str,
    values: &[f32],
) {
    let at = timestamp("2035-01-01T00:00:01Z");
    archive
        .upsert_document_fenced(lease, document, at)
        .await
        .expect("store current document");
    let vector = EmbeddingVector::new(values.to_vec(), None).expect("valid embedding vector");
    archive
        .upsert_embedding_chunk_fenced(
            lease,
            document,
            &EmbeddingChunkInput {
                endpoint,
                model,
                index: 0,
                count: 1,
                chunk_hash: &document.content_hash,
                vector: &vector,
            },
            at,
        )
        .await
        .expect("store current embedding");
}

/// Reserves evidence and commits an open issue with a complete thread observation.
///
/// Title numbering comes from the thread identity, so setup cannot accidentally pair a thread
/// with another issue number. The provider update time also supplies this fixture acquisition time.
async fn apply_thread(archive: &Archive, thread: &ThreadId, updated_at: UtcTimestamp) {
    let sequence = archive
        .reserve_observation_sequence(updated_at)
        .await
        .expect("reserve observation sequence");
    let title = format!("Shared cache failure {}", thread.number().get());
    let discussion = Discussion {
        id: thread.clone(),
        kind: ThreadKind::Issue,
        state: SourceState::Open,
        title,
        body: Some("The cache fails after restart.".to_owned()),
        html_url: None,
        created_at: timestamp("2026-09-19T10:00:00Z"),
        updated_at,
        closed_at: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        provider_data: ProviderData::new(),
    };
    let raw_clock = updated_at.format_rfc3339().expect("format source clock");
    archive
        .apply_thread_observation(&Observation::new(
            EvidenceFamily::Threads,
            discussion,
            SourceClock::from_raw(Some(&raw_clock)),
            updated_at,
            sequence,
            CollectionCompleteness::Complete,
        ))
        .await
        .expect("apply thread observation");
}

/// Constructs the synthetic clustering repository without registering it in an archive.
fn repository() -> Repository {
    Repository {
        id: RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new("repo-cluster-workflow").expect("repository provider ID"),
        ),
        owner: "example".to_owned(),
        name: "clustering".to_owned(),
        full_name: "example/clustering".to_owned(),
        default_branch: Some("main".to_owned()),
        updated_at: Some(timestamp("2026-09-20T10:00:00Z")),
        provider_data: ProviderData::new(),
    }
}

/// Constructs a repository-scoped issue identity with the scenario-selected provider ID and number.
fn thread_id(repository: &RepositoryId, provider_id: &str, number: u64) -> ThreadId {
    ThreadId::new(
        repository.clone(),
        ProviderId::new(provider_id).expect("thread provider ID"),
        ThreadNumber::new(number).expect("thread number"),
    )
}

/// Parses a fixture clock value and reports invalid setup before workflow execution.
fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
}

/// Allocates a process-local unique filename without creating or opening an archive.
fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-engine-clusters-{}-{sequence}.sqlite",
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
