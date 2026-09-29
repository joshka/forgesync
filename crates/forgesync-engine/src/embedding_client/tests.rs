use std::time::Duration;

use reqwest::Url;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{EmbeddingClient, EmbeddingClientConfig, EmbeddingClientError, validate_response};
use crate::embedding_client::EmbeddingResponse;

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

#[tokio::test]
async fn compatible_embedding_request_orders_responses_by_index() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .and(header("authorization", "Bearer fixture-secret"))
        .and(body_string_contains("\"model\":\"fixture-model\""))
        .and(body_string_contains("\"dimensions\":2"))
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

    assert_eq!(result[0].values(), &[1.0, 0.0]);
    assert_eq!(result[1].values(), &[0.0, 1.0]);
    assert_eq!(server.received_requests().await.expect("requests").len(), 1);
}

#[tokio::test]
async fn malformed_indices_and_vectors_are_rejected() {
    let responses = [
        json!({"data": [{"index": 0, "embedding": [1.0, 0.0]}]}),
        json!({"data": [
            {"index": 0, "embedding": [1.0, 0.0]},
            {"index": 0, "embedding": [0.0, 1.0]}
        ]}),
        json!({"data": [
            {"index": 0, "embedding": [1.0, 0.0]},
            {"index": 2, "embedding": [0.0, 1.0]}
        ]}),
        json!({"data": [
            {"index": 0, "embedding": [0.0, 0.0]},
            {"index": 1, "embedding": [0.0, 1.0]}
        ]}),
        json!({"data": [
            {"index": 0, "embedding": [1.0]},
            {"index": 1, "embedding": [0.0, 1.0]}
        ]}),
        json!({"data": [
            {"index": 0, "embedding": [3.5e39, 0.0]},
            {"index": 1, "embedding": [0.0, 1.0]}
        ]}),
    ];
    for response in responses {
        let result = validate_response(
            serde_json::from_value::<EmbeddingResponse>(response).expect("typed fixture"),
            2,
            Some(2),
            "fixture-model",
        );
        assert!(result.is_err());
    }
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
    assert!(
        destination
            .received_requests()
            .await
            .expect("requests")
            .is_empty()
    );
}
