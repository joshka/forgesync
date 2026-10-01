//! Explicit keyword fallback avoids a missing-key service request.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::embeddings::embed_documents;
use forgesync_engine::inspect::{ThreadFilters, ThreadSort, ThreadStateFilter};
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::search::{SearchMode, SearchProvenance, SearchRequest, retrieve_threads};
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
async fn missing_key_falls_back_without_a_query_vector_request() {
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
        fail_on_call: AtomicUsize::new(0),
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

    let embedded = embed_documents(
        &archive,
        &client,
        std::slice::from_ref(&document),
        forgesync_engine::embeddings::EmbeddingPolicy::Missing,
        &CancellationToken::new(),
    )
    .await
    .expect("prepare complete current vectors");
    assert_eq!(embedded.chunks_embedded, 5);
    assert!(embedded.failed_batches.is_empty());
    assert_eq!(responder.calls.load(Ordering::SeqCst), 5);

    let no_key_client = single_input_client(&server, "");
    let fallback = retrieve_threads(
        &archive,
        &SearchRequest {
            query: "target".to_owned(),
            mode: SearchMode::Hybrid,
            filters: ThreadFilters {
                repositories: vec!["owner/repo".parse().expect("repository selector")],
                kind: None,
                state: ThreadStateFilter::All,
                sort: Some(ThreadSort::Relevance),
                limit: 10,
                offset: 0,
            },
            allow_keyword_fallback: true,
        },
        DocumentRecipe::OriginalBody,
        Some(&no_key_client),
        &CancellationToken::new(),
    )
    .await
    .expect("explicit keyword fallback");
    assert_eq!(fallback.requested_mode, SearchMode::Hybrid);
    assert_eq!(fallback.mode, SearchMode::Keyword);
    assert_eq!(
        fallback.fallback_reason.as_deref(),
        Some("embedding_key_missing")
    );
    assert_eq!(fallback.items.len(), 1);
    assert_eq!(fallback.items[0].summary.discussion.id.number().get(), 11);
    assert_eq!(
        fallback.items[0].provenance,
        vec![SearchProvenance::Keyword { rank: 1 }]
    );
    assert_eq!(responder.calls.load(Ordering::SeqCst), 5);

    archive.close().await;
    remove_archive(&archive_path);
}
