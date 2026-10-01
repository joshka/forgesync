//! Retry fills missing chunks without changing committed vectors.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::embeddings::embed_documents;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use super::fixture_archive::{
    current_timestamp, remove_archive, temporary_archive_path, thread_reference,
};
use super::fixture_embeddings::{
    EmbeddingResponder, SharedEmbeddingResponder, single_input_client,
};
use super::fixture_issues::{
    clients_for, issue_with_comment_count, mount_open_issues, mount_repository,
};

#[tokio::test]
async fn embedding_retry_keeps_successful_batches_and_requests_only_missing_chunks() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    mount_open_issues(
        &server,
        vec![issue_with_comment_count(
            91,
            11,
            "Embedding target",
            "2026-09-20T09:30:00Z",
            0,
        )],
    )
    .await;
    let responder = Arc::new(EmbeddingResponder {
        calls: AtomicUsize::new(0),
        fail_on_call: AtomicUsize::new(2),
    });
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(SharedEmbeddingResponder(Arc::clone(&responder)))
        .mount(&server)
        .await;

    let archive_path = temporary_archive_path();
    let archive = Archive::create(&archive_path)
        .await
        .expect("create archive");
    let repository = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("repository selector");
    let clients = clients_for(&server, &repository);
    let request = SyncRequest {
        repositories: vec![repository],
        all: false,
        scope: SyncThreadScope::Open,
        include_comments: false,
        include_reviews: false,
        include_review_threads: false,
        parent_run: None,
    };
    let sync = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert_eq!(sync.outcome, OperationOutcome::Complete);
    let thread_11_detail = archive
        .thread_detail(&thread_reference(11))
        .await
        .expect("read current thread 11 detail");
    let thread = thread_11_detail.summary;
    let now = current_timestamp();
    let text = "alpha beta gamma delta epsilon".to_owned();
    let document = Document::new(
        thread.discussion.id.clone(),
        DocumentRecipe::OriginalBody,
        thread.discussion.title.clone(),
        text.clone(),
        text.to_lowercase(),
        thread.discussion.updated_at,
    );
    let lease = archive
        .acquire_archive_lease(now, Duration::from_secs(60))
        .await
        .expect("claim archive lease for document");
    archive
        .upsert_document_fenced(&lease, &document, now)
        .await
        .expect("store embedding source document");
    archive
        .release_archive_lease(&lease, current_timestamp())
        .await
        .expect("release document lease");

    let client = single_input_client(&server, "fixture-key");

    let first = embed_documents(
        &archive,
        &client,
        std::slice::from_ref(&document),
        forgesync_engine::embeddings::EmbeddingPolicy::Missing,
        &CancellationToken::new(),
    )
    .await
    .expect("first embedding run");
    assert_eq!(first.chunks_selected, 5);
    assert_eq!(first.chunks_embedded, 4);
    assert_eq!(first.failed_batches.len(), 1);
    let retained = archive
        .embedding_chunks(&document, client.endpoint_identity(), client.model(), 5)
        .await
        .expect("read committed chunks");
    assert_eq!(retained.len(), 4);
    assert_eq!(
        [
            retained[0].index,
            retained[1].index,
            retained[2].index,
            retained[3].index
        ],
        [0, 2, 3, 4]
    );

    responder.fail_on_call.store(0, Ordering::SeqCst);
    let retried = embed_documents(
        &archive,
        &client,
        std::slice::from_ref(&document),
        forgesync_engine::embeddings::EmbeddingPolicy::Missing,
        &CancellationToken::new(),
    )
    .await
    .expect("retry missing chunk");
    assert_eq!(retried.chunks_skipped, 4);
    assert_eq!(retried.chunks_embedded, 1);
    assert!(retried.failed_batches.is_empty());
    assert_eq!(responder.calls.load(Ordering::SeqCst), 6);
    let completed = archive
        .embedding_chunks(&document, client.endpoint_identity(), client.model(), 5)
        .await
        .expect("read completed chunks");
    assert_eq!(completed.len(), 5);
    assert_eq!(completed[0], retained[0]);
    assert_eq!(completed[2], retained[1]);
    assert_eq!(completed[3], retained[2]);
    assert_eq!(completed[4], retained[3]);
    assert_eq!(completed[1].index, 1);
    assert_eq!(completed[1].count, 5);
    assert_eq!(completed[1].vector.values(), &[0.6, 0.8]);

    archive.close().await;
    remove_archive(&archive_path);
}
