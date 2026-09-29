//! Embeddings workflow contracts.

use super::{
    Arc, Archive, AtomicUsize, CancellationToken, Document, DocumentRecipe, Duration,
    EmbeddingClient, EmbeddingClientConfig, Mock, MockServer, OperationOutcome, Ordering,
    RepositorySelector, Request, Respond, ResponseTemplate, SearchMode, SearchRanking,
    SearchRequest, SyncThreadScope, ThreadFilters, ThreadSort, ThreadStateFilter,
    current_timestamp, embed_documents, issue_with_comment_count, json, method, mount_open_issues,
    mount_repository, path, remove_archive, retrieve_threads, sync_once, temporary_archive_path,
    thread_summary,
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
    let sync = sync_once(&archive, &server, repository, SyncThreadScope::Open).await;
    assert_eq!(sync.outcome, OperationOutcome::Complete);
    let thread = thread_summary(&archive, 11).await;
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

    let client = EmbeddingClient::new(EmbeddingClientConfig {
        endpoint: format!("{}/v1", server.uri()).parse().expect("endpoint"),
        model: "fixture-model".to_owned(),
        api_key: "fixture-key".to_owned(),
        dimensions: Some(2),
        max_input_bytes: 10,
        max_batch_input_bytes: 10,
        batch_size: 1,
        concurrency: 1,
        request_timeout: Duration::from_secs(2),
        total_budget: Duration::from_secs(3),
        max_attempts: 1,
    })
    .expect("embedding client");

    let first = embed_documents(
        &archive,
        &client,
        std::slice::from_ref(&document),
        false,
        &CancellationToken::new(),
    )
    .await
    .expect("first embedding run");
    assert_eq!(first.chunks_selected, 5);
    assert_eq!(first.chunks_embedded, 4);
    assert_eq!(first.failed_batches.len(), 1);
    assert_eq!(
        archive
            .embedding_chunks(&document, client.endpoint_identity(), client.model(), 5)
            .await
            .expect("read committed chunks")
            .len(),
        4
    );

    responder.fail_on_call.store(0, Ordering::SeqCst);
    let retried = embed_documents(
        &archive,
        &client,
        std::slice::from_ref(&document),
        false,
        &CancellationToken::new(),
    )
    .await
    .expect("retry missing chunk");
    assert_eq!(retried.chunks_skipped, 4);
    assert_eq!(retried.chunks_embedded, 1);
    assert!(retried.failed_batches.is_empty());
    assert_eq!(responder.calls.load(Ordering::SeqCst), 6);
    assert_eq!(
        archive
            .embedding_chunks(&document, client.endpoint_identity(), client.model(), 5)
            .await
            .expect("read complete chunks")
            .len(),
        5
    );

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
    assert_eq!(hybrid.ranking, SearchRanking::ReciprocalRankFusion);
    assert_eq!(hybrid.items.len(), 1);
    assert_eq!(hybrid.items[0].summary.discussion.id.number().get(), 11);
    assert_eq!(hybrid.items[0].provenance.len(), 2);
    assert!((hybrid.items[0].score.expect("RRF score") - 2.0 / 61.0).abs() < f64::EPSILON);
    assert_eq!(responder.calls.load(Ordering::SeqCst), 7);

    let no_key_client = EmbeddingClient::new(EmbeddingClientConfig {
        endpoint: format!("{}/v1", server.uri()).parse().expect("endpoint"),
        model: "fixture-model".to_owned(),
        api_key: String::new(),
        dimensions: Some(2),
        max_input_bytes: 10,
        max_batch_input_bytes: 10,
        batch_size: 1,
        concurrency: 1,
        request_timeout: Duration::from_secs(2),
        total_budget: Duration::from_secs(3),
        max_attempts: 1,
    })
    .expect("client without a resolved key");
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
    assert_eq!(responder.calls.load(Ordering::SeqCst), 7);

    archive.close().await;
    remove_archive(&archive_path);
}

struct EmbeddingResponder {
    calls: AtomicUsize,
    fail_on_call: AtomicUsize,
}

struct SharedEmbeddingResponder(Arc<EmbeddingResponder>);

impl Respond for SharedEmbeddingResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.0.respond(request)
    }
}

impl Respond for EmbeddingResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call == self.fail_on_call.load(Ordering::SeqCst) {
            ResponseTemplate::new(503)
        } else {
            ResponseTemplate::new(200).set_body_json(json!({
                "data": [{"index": 0, "embedding": [0.6, 0.8]}],
                "model": "fixture-model",
                "object": "list"
            }))
        }
    }
}
