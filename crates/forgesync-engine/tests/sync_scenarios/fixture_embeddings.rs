//! # Embedding service response policy and client construction
//!
//! These fixtures supply a sequential single-input service: every successful request returns one
//! fixed two-dimensional vector, while a caller-selected one-based call can fail with HTTP 503.
//! Atomic counters make observed HTTP attempts and fault selection explicit to each scenario.
//!
//! A zero failure ordinal means no failure. Concurrency one and batch size one make the ordinal
//! correspond to one chunk request in retry tests. Search scenarios leave failure disabled and use
//! the same responder to count the additional query-vector request or prove its absence.
//!
//! `single_input_client` constructs configuration with ten-byte chunks, one attempt, fixture model,
//! and the caller's resolved key. No archive reads, writes, source acquisition, or embeddings occur
//! during construction; scenarios mount the responder and invoke real operations themselves.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forgesync_engine::embedding_client::{EmbeddingClient, EmbeddingClientConfig};
use serde_json::json;
use wiremock::{MockServer, Request, Respond, ResponseTemplate};

/// Counts attempts and injects one ordinal failure without deciding expected scenario results.
pub struct EmbeddingResponder {
    /// Total HTTP attempts, including failures and query-vector requests.
    pub calls: AtomicUsize,
    /// One-based attempt returning 503; zero disables fault injection.
    pub fail_on_call: AtomicUsize,
}

/// Shares responder counters with the scenario after mounting it on the server.
pub struct SharedEmbeddingResponder(pub Arc<EmbeddingResponder>);

impl Respond for SharedEmbeddingResponder {
    /// Delegates a received request to the shared ordinal response policy.
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.0.respond(request)
    }
}

impl Respond for EmbeddingResponder {
    /// Counts this attempt and returns its configured failure or fixed valid vector.
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

/// Constructs a ten-byte, one-input, sequential client without making a service request.
///
/// The key is supplied explicitly so fallback scenarios can distinguish preparation credentials
/// from an unresolved query credential. A single attempt prevents hidden HTTP retries in counters.
pub fn single_input_client(server: &MockServer, key: &str) -> EmbeddingClient {
    let endpoint = format!("{}/v1", server.uri())
        .parse()
        .expect("fixture endpoint");
    let config = EmbeddingClientConfig {
        endpoint,
        model: "fixture-model".to_owned(),
        api_key: key.to_owned(),
        dimensions: Some(2),
        max_input_bytes: 10,
        max_batch_input_bytes: 10,
        batch_size: 1,
        concurrency: 1,
        request_timeout: Duration::from_secs(2),
        total_budget: Duration::from_secs(3),
        max_attempts: 1,
    };
    EmbeddingClient::new(config).expect("fixture embedding client")
}
