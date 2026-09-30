//! # Embedding protocol and validation
//!
//! These tests cover ordered vector responses, malformed indexes or values, and a credential-
//! bearing redirect. They use a local server or response fixture to show the adapter behavior at
//! the HTTP boundary. A successful status is not enough: the response must match the requested
//! inputs before the engine stores vectors. Preserve the redirect case when changing client
//! configuration because authentication must not be forwarded to a different destination.
//!
//! The retry case declares a one-shot server failure followed by a successful response. Direct
//! backoff cases establish budget rejection and cancellation without making a provider request.
//! The protocol scenario matches the full JSON request, including input order, and checks the
//! returned vector count before comparing indexed values. Redirect assertions inspect both source
//! and destination request histories so rejection is tied to one actual source attempt.
//!
//! Configuration fixtures only construct settings. Request execution, retry policy overrides, and
//! mock expectations stay beside the operation and result. Response-shape rejection has its own
//! named fixture suite; these transport cases do not duplicate its validation matrix.

use std::time::{Duration, Instant};

use reqwest::Url;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::embedding_client::{EmbeddingClient, EmbeddingClientConfig, EmbeddingClientError};

#[tokio::test]
async fn compatible_embedding_request_orders_responses_by_index() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .and(header("authorization", "Bearer fixture-secret"))
        .and(body_json(json!({
            "model": "fixture-model",
            "dimensions": 2,
            "encoding_format": "float",
            "input": ["first", "second"]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {"index": 1, "embedding": [0.0, 1.0]},
                {"index": 0, "embedding": [1.0, 0.0]}
            ],
            "model": "fixture-model",
            "object": "list"
        })))
        .mount(&server)
        .await;
    let client = EmbeddingClient::new(config(&format!("{}/v1", server.uri()))).expect("client");
    let result = client
        .embed(
            &["first".to_owned(), "second".to_owned()],
            &CancellationToken::new(),
        )
        .await
        .expect("embedding vectors");

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].values(), &[1.0, 0.0]);
    assert_eq!(result[1].values(), &[0.0, 1.0]);
    let requests = server.received_requests().await.expect("recorded requests");
    assert_eq!(requests.len(), 1);
}

#[tokio::test]
async fn authenticated_redirect_is_rejected_without_forwarding_the_key() {
    let destination = MockServer::start().await;
    let source = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{}/capture", destination.uri())),
        )
        .mount(&source)
        .await;
    let client = EmbeddingClient::new(config(&format!("{}/v1", source.uri()))).expect("client");
    let result = client
        .embed(&["text".to_owned()], &CancellationToken::new())
        .await;

    assert_eq!(result, Err(EmbeddingClientError::RedirectRejected));
    let source_requests = source.received_requests().await.expect("source requests");
    let destination_requests = destination
        .received_requests()
        .await
        .expect("destination requests");
    assert_eq!(source_requests.len(), 1);
    assert!(destination_requests.is_empty());
}

#[tokio::test]
async fn retryable_server_failure_is_followed_by_one_successful_attempt() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(503))
        .with_priority(1)
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{"index": 0, "embedding": [1.0, 0.0]}]
        })))
        .with_priority(2)
        .expect(1)
        .mount(&server)
        .await;
    let mut settings = config(&format!("{}/v1", server.uri()));
    settings.max_attempts = 2;
    let client = EmbeddingClient::new(settings).expect("client");

    let vectors = client
        .embed(&["text".to_owned()], &CancellationToken::new())
        .await
        .expect("retry succeeds");

    assert_eq!(vectors.len(), 1);
    assert_eq!(vectors[0].values(), &[1.0, 0.0]);
    server.verify().await;
}

#[tokio::test]
async fn backoff_that_consumes_the_remaining_budget_is_rejected() {
    let mut settings = config("http://127.0.0.1:1/v1");
    settings.total_budget = Duration::from_millis(200);
    let client = EmbeddingClient::new(settings).expect("client");
    let started = Instant::now();

    let result = client
        .wait_to_retry(0, started, &CancellationToken::new())
        .await;

    assert_eq!(result, Err(EmbeddingClientError::RetryBudgetExhausted));
}

#[tokio::test]
async fn cancelled_backoff_returns_cancellation_without_waiting_for_another_attempt() {
    let client = EmbeddingClient::new(config("http://127.0.0.1:1/v1")).expect("client");
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let started = Instant::now();

    let result = client.wait_to_retry(0, started, &cancellation).await;

    assert_eq!(result, Err(EmbeddingClientError::Cancelled));
}

/// Builds a two-dimensional, two-input adapter configuration with fixture credentials.
///
/// Construction performs no I/O. The default permits one attempt; retry cases change that field
/// explicitly, and direct backoff cases use an unreachable loopback endpoint without sending data.
fn config(endpoint: &str) -> EmbeddingClientConfig {
    EmbeddingClientConfig {
        endpoint: Url::parse(endpoint).expect("endpoint"),
        model: "fixture-model".to_owned(),
        api_key: "fixture-secret".to_owned(),
        dimensions: Some(2),
        max_input_bytes: 32,
        max_batch_input_bytes: 64,
        batch_size: 2,
        concurrency: 2,
        request_timeout: Duration::from_secs(2),
        total_budget: Duration::from_secs(3),
        max_attempts: 1,
    }
}
