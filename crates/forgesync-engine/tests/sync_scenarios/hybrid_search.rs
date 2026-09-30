//! # Hybrid retrieval combines keyword and semantic evidence
//!
//! A real parent sync and explicit document write establish a matching title and current document.
//! Five successful service requests prepare its complete vector set; the query then makes one
//! further service request for its semantic representation.
//!
//! The result retains hybrid mode without fallback and identifies the expected thread with both
//! provenance paths. Reciprocal-rank fusion combines two first-place contributions into the
//! expected score. This test does not first manufacture a failed embedding batch to reach retrieval
//! setup.
//!
//! Requests, fenced persistence, embedding preparation, and retrieval are explicit. The shared
//! fixture configures client limits and supplies fixed vectors without performing archive work.
//! Retry preservation and unauthenticated fallback remain independent sibling contracts.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::embeddings::embed_documents;
use forgesync_engine::inspect::{ThreadFilters, ThreadSort, ThreadStateFilter};
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::search::{
    SearchMode, SearchProvenance, SearchRanking, SearchRequest, retrieve_threads,
};
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
async fn hybrid_retrieval_combines_keyword_and_semantic_evidence() {
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

    let hybrid = retrieve_threads(
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
            allow_keyword_fallback: false,
        },
        DocumentRecipe::OriginalBody,
        Some(&client),
        &CancellationToken::new(),
    )
    .await
    .expect("hybrid retrieval");
    assert_eq!(hybrid.requested_mode, SearchMode::Hybrid);
    assert_eq!(hybrid.mode, SearchMode::Hybrid);
    assert_eq!(hybrid.fallback_reason, None);
    assert_eq!(hybrid.ranking, SearchRanking::ReciprocalRankFusion);
    assert_eq!(hybrid.items.len(), 1);
    assert_eq!(hybrid.items[0].summary.discussion.id.number().get(), 11);
    assert!(matches!(
        hybrid.items[0].provenance.as_slice(),
        [
            SearchProvenance::Keyword { rank: 1 },
            SearchProvenance::Semantic { rank: 1, .. }
        ]
    ));
    let fusion_score = hybrid.items[0].score.expect("RRF score");
    assert!((fusion_score - 2.0 / 61.0).abs() < f64::EPSILON);
    assert_eq!(responder.calls.load(Ordering::SeqCst), 6);

    archive.close().await;
    remove_archive(&archive_path);
}
