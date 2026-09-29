use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use forgesync_core::embedding::{EmbeddingVector, EmbeddingVectorError};
use reqwest::Url;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

const MAX_RESPONSE_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_BATCH_INPUTS: usize = 2048;
const MAX_EMBEDDING_DIMENSIONS: usize = 65_536;
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);

/// Independent endpoint and capacity settings for one embedding service.
#[derive(Clone)]
pub struct EmbeddingClientConfig {
    /// Base URL before the standard `/embeddings` path is appended.
    pub endpoint: Url,
    /// Requested model name.
    pub model: String,
    /// Secret API key resolved by the application boundary.
    pub api_key: String,
    /// Expected vector dimensions, when the model uses a configured dimension count.
    pub dimensions: Option<u32>,
    /// Maximum UTF-8 bytes in one deterministic document chunk.
    pub max_input_bytes: usize,
    /// Maximum UTF-8 bytes sent across one request batch.
    pub max_batch_input_bytes: usize,
    /// Maximum chunks in one provider request.
    pub batch_size: usize,
    /// Maximum requests in flight for this service.
    pub concurrency: usize,
    /// Timeout for one provider request.
    pub request_timeout: Duration,
    /// End-to-end budget covering attempts and retry delays.
    pub total_budget: Duration,
    /// Maximum attempts for transient network, rate-limit, and server failures.
    pub max_attempts: u32,
}

/// OpenAI-compatible embeddings client with bounded requests and strict response validation.
#[derive(Clone)]
pub struct EmbeddingClient {
    http: reqwest::Client,
    endpoint: Url,
    endpoint_identity: String,
    model: String,
    api_key: Arc<str>,
    dimensions: Option<u32>,
    max_input_bytes: usize,
    max_batch_input_bytes: usize,
    batch_size: usize,
    concurrency: usize,
    total_budget: Duration,
    max_attempts: u32,
    request_slots: Arc<Semaphore>,
}

impl EmbeddingClient {
    /// Builds a reusable client without allowing API credentials across redirects.
    pub fn new(config: EmbeddingClientConfig) -> Result<Self, EmbeddingClientError> {
        validate_config(&config)?;
        let endpoint_identity = config.endpoint.as_str().trim_end_matches('/').to_owned();
        let mut base = config.endpoint;
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        let endpoint = base
            .join("embeddings")
            .map_err(|_| EmbeddingClientError::InvalidConfiguration)?;
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("forgesync")
            .build()
            .map_err(|_| EmbeddingClientError::ClientInitialization)?;

        Ok(Self {
            http,
            endpoint,
            endpoint_identity,
            model: config.model.trim().to_owned(),
            api_key: Arc::from(config.api_key.trim()),
            dimensions: config.dimensions,
            max_input_bytes: config.max_input_bytes,
            max_batch_input_bytes: config.max_batch_input_bytes,
            batch_size: config.batch_size,
            concurrency: config.concurrency,
            total_budget: config.total_budget,
            max_attempts: config.max_attempts,
            request_slots: Arc::new(Semaphore::new(config.concurrency)),
        })
    }

    /// Canonical base endpoint identity stored with the vectors, without credentials.
    pub fn endpoint_identity(&self) -> &str {
        &self.endpoint_identity
    }

    /// Configured model name stored with the vectors.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Maximum bytes allowed in an individual provider input.
    pub const fn max_input_bytes(&self) -> usize {
        self.max_input_bytes
    }

    /// Maximum bytes allowed in a provider request batch.
    pub const fn max_batch_input_bytes(&self) -> usize {
        self.max_batch_input_bytes
    }

    /// Maximum inputs allowed in one provider request.
    pub const fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// Maximum parallel provider requests for this service.
    pub fn concurrency(&self) -> usize {
        self.concurrency
    }

    /// Configured output dimension count, when known.
    pub const fn dimensions(&self) -> Option<u32> {
        self.dimensions
    }

    /// Maximum time one batch can hold the writer lease while provider retries run.
    pub const fn request_budget(&self) -> Duration {
        self.total_budget
    }

    /// Requests one vector per input and validates every returned index and component.
    pub async fn embed(
        &self,
        inputs: &[String],
        cancellation: &CancellationToken,
    ) -> Result<Vec<EmbeddingVector>, EmbeddingClientError> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        if self.api_key.trim().is_empty() {
            return Err(EmbeddingClientError::MissingApiKey);
        }
        self.validate_inputs(inputs)?;
        let _permit = self.acquire_request_slot(cancellation).await?;
        let started = Instant::now();

        for attempt in 0..self.max_attempts {
            if cancellation.is_cancelled() {
                return Err(EmbeddingClientError::Cancelled);
            }
            let remaining = self.total_budget.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(EmbeddingClientError::RetryBudgetExhausted);
            }
            let request = self.request_once(inputs, cancellation);
            let result = tokio::select! {
                _ = cancellation.cancelled() => return Err(EmbeddingClientError::Cancelled),
                result = tokio::time::timeout(remaining, request) => match result {
                    Ok(result) => result,
                    Err(_) => return Err(EmbeddingClientError::RetryBudgetExhausted),
                },
            };
            match result {
                Ok(vectors) => return Ok(vectors),
                Err(error) if error.retryable() && attempt + 1 < self.max_attempts => {
                    let delay = retry_delay(attempt);
                    if delay >= self.total_budget.saturating_sub(started.elapsed()) {
                        return Err(EmbeddingClientError::RetryBudgetExhausted);
                    }
                    tokio::select! {
                        _ = cancellation.cancelled() => return Err(EmbeddingClientError::Cancelled),
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Err(EmbeddingClientError::RetryBudgetExhausted)
    }

    async fn acquire_request_slot(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<OwnedSemaphorePermit, EmbeddingClientError> {
        tokio::select! {
            _ = cancellation.cancelled() => Err(EmbeddingClientError::Cancelled),
            permit = Arc::clone(&self.request_slots).acquire_owned() => {
                permit.map_err(|_| EmbeddingClientError::ConcurrencyUnavailable)
            }
        }
    }

    async fn request_once(
        &self,
        inputs: &[String],
        cancellation: &CancellationToken,
    ) -> Result<Vec<EmbeddingVector>, EmbeddingClientError> {
        let request = EmbeddingRequest {
            model: &self.model,
            input: inputs,
            dimensions: self.dimensions,
            encoding_format: "float",
        };
        let body =
            serde_json::to_vec(&request).map_err(|_| EmbeddingClientError::InvalidRequest)?;
        let mut authorization = HeaderValue::from_str(&format!("Bearer {}", self.api_key))
            .map_err(|_| EmbeddingClientError::InvalidConfiguration)?;
        authorization.set_sensitive(true);
        let response = self
            .http
            .post(self.endpoint.clone())
            .header(AUTHORIZATION, authorization)
            .header(CONTENT_TYPE, "application/json")
            .header(USER_AGENT, "forgesync")
            .body(body)
            .send()
            .await
            .map_err(map_request_error)?;
        let status = response.status();
        if status.is_redirection() {
            return Err(EmbeddingClientError::RedirectRejected);
        }
        if !status.is_success() {
            return Err(EmbeddingClientError::ApiStatus(status.as_u16()));
        }
        let bytes = read_bounded_body(response, cancellation).await?;
        let response: EmbeddingResponse =
            serde_json::from_slice(&bytes).map_err(|_| EmbeddingClientError::InvalidResponse)?;
        validate_response(response, inputs.len(), self.dimensions, &self.model)
    }

    fn validate_inputs(&self, inputs: &[String]) -> Result<(), EmbeddingClientError> {
        if inputs.len() > self.batch_size || inputs.len() > MAX_BATCH_INPUTS {
            return Err(EmbeddingClientError::BatchTooLarge);
        }
        let mut total_bytes = 0usize;
        for input in inputs {
            if input.trim().is_empty() {
                return Err(EmbeddingClientError::EmptyInput);
            }
            if input.len() > self.max_input_bytes {
                return Err(EmbeddingClientError::InputTooLarge);
            }
            total_bytes = total_bytes
                .checked_add(input.len())
                .ok_or(EmbeddingClientError::BatchTooLarge)?;
        }
        if total_bytes > self.max_batch_input_bytes {
            return Err(EmbeddingClientError::BatchTooLarge);
        }
        Ok(())
    }
}

/// Sanitized transport, response, and vector validation failure.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum EmbeddingClientError {
    /// Endpoint, model, key, dimensions, or service limits are invalid.
    #[error("embedding service configuration is invalid")]
    InvalidConfiguration,
    /// The HTTP client could not be initialized.
    #[error("embedding HTTP client initialization failed")]
    ClientInitialization,
    /// The JSON request could not be encoded.
    #[error("embedding request could not be encoded")]
    InvalidRequest,
    /// The configured environment variable did not contain an API key.
    #[error("embedding API key is not set")]
    MissingApiKey,
    /// A request input is empty.
    #[error("embedding input is empty")]
    EmptyInput,
    /// A request input exceeds the configured byte budget.
    #[error("embedding input exceeds the configured byte budget")]
    InputTooLarge,
    /// A request batch exceeds configured count or byte budgets.
    #[error("embedding request batch exceeds its configured budget")]
    BatchTooLarge,
    /// The request was cancelled by its caller.
    #[error("embedding request was cancelled")]
    Cancelled,
    /// The request could not acquire configured concurrency.
    #[error("embedding request capacity is unavailable")]
    ConcurrencyUnavailable,
    /// A redirect was rejected to protect the configured credential.
    #[error("embedding service redirected the authenticated request")]
    RedirectRejected,
    /// The endpoint returned a non-success HTTP status.
    #[error("embedding service returned HTTP {0}")]
    ApiStatus(u16),
    /// The request failed because of a network error.
    #[error("embedding request failed due to a network error")]
    Network,
    /// The request exceeded its configured request timeout.
    #[error("embedding request timed out")]
    Timeout,
    /// Automatic attempts exceeded the total time budget.
    #[error("embedding retry budget was exhausted")]
    RetryBudgetExhausted,
    /// The successful response body exceeded the local size limit.
    #[error("embedding response exceeded the configured body limit")]
    ResponseTooLarge,
    /// The successful response body was not valid JSON or did not match the request.
    #[error("embedding service response is invalid")]
    InvalidResponse,
    /// The returned vector failed numeric or dimension validation.
    #[error("embedding vector is invalid")]
    InvalidVector,
}

impl EmbeddingClientError {
    /// Stable machine-readable classification for CLI and run reports.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "embedding_config_invalid",
            Self::ClientInitialization => "embedding_client_unavailable",
            Self::InvalidRequest => "embedding_request_invalid",
            Self::MissingApiKey => "embedding_key_missing",
            Self::EmptyInput => "embedding_input_empty",
            Self::InputTooLarge => "embedding_input_too_large",
            Self::BatchTooLarge => "embedding_batch_too_large",
            Self::Cancelled => "embedding_cancelled",
            Self::ConcurrencyUnavailable => "embedding_concurrency_unavailable",
            Self::RedirectRejected => "embedding_redirect_rejected",
            Self::ApiStatus(_) => "embedding_api_error",
            Self::Network => "embedding_network_error",
            Self::Timeout => "embedding_timeout",
            Self::RetryBudgetExhausted => "embedding_retry_exhausted",
            Self::ResponseTooLarge => "embedding_response_too_large",
            Self::InvalidResponse => "embedding_response_invalid",
            Self::InvalidVector => "embedding_vector_invalid",
        }
    }

    fn retryable(self) -> bool {
        match self {
            Self::Network | Self::Timeout => true,
            Self::ApiStatus(status) => status == 429 || status >= 500,
            _ => false,
        }
    }
}

impl From<EmbeddingVectorError> for EmbeddingClientError {
    fn from(_: EmbeddingVectorError) -> Self {
        Self::InvalidVector
    }
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<u32>,
    encoding_format: &'static str,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingResponseItem>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponseItem {
    index: usize,
    embedding: Vec<f64>,
}

fn validate_config(config: &EmbeddingClientConfig) -> Result<(), EmbeddingClientError> {
    if !valid_endpoint(&config.endpoint)
        || config.model.trim().is_empty()
        || config.dimensions == Some(0)
        || config
            .dimensions
            .is_some_and(|dimensions| dimensions > 65_536)
        || config.max_input_bytes < 4
        || config.max_batch_input_bytes < config.max_input_bytes
        || config.max_batch_input_bytes > 300_000
        || config.batch_size == 0
        || config.batch_size > MAX_BATCH_INPUTS
        || config.concurrency == 0
        || config.concurrency > 64
        || config.request_timeout.is_zero()
        || config.request_timeout > Duration::from_secs(600)
        || config.total_budget.is_zero()
        || config.total_budget > Duration::from_secs(3600)
        || config.max_attempts == 0
        || config.max_attempts > 8
    {
        return Err(EmbeddingClientError::InvalidConfiguration);
    }
    Ok(())
}

fn valid_endpoint(endpoint: &Url) -> bool {
    let secure = endpoint.scheme() == "https";
    let local_http =
        endpoint.scheme() == "http" && endpoint.host_str().is_some_and(is_loopback_host);
    (secure || local_http)
        && endpoint.host_str().is_some()
        && endpoint.username().is_empty()
        && endpoint.password().is_none()
        && endpoint.query().is_none()
        && endpoint.fragment().is_none()
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn retry_delay(attempt: u32) -> Duration {
    let exponent = attempt.min(5);
    Duration::from_millis(200_u64.saturating_mul(1_u64 << exponent)).min(MAX_RETRY_DELAY)
}

fn map_request_error(error: reqwest::Error) -> EmbeddingClientError {
    if error.is_timeout() {
        EmbeddingClientError::Timeout
    } else {
        EmbeddingClientError::Network
    }
}

async fn read_bounded_body(
    mut response: reqwest::Response,
    cancellation: &CancellationToken,
) -> Result<Vec<u8>, EmbeddingClientError> {
    let mut body = Vec::new();
    loop {
        let next = tokio::select! {
            _ = cancellation.cancelled() => return Err(EmbeddingClientError::Cancelled),
            next = response.chunk() => next.map_err(map_request_error)?,
        };
        let Some(chunk) = next else {
            return Ok(body);
        };
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BODY_BYTES {
            return Err(EmbeddingClientError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
}

fn validate_response(
    response: EmbeddingResponse,
    input_count: usize,
    expected_dimensions: Option<u32>,
    expected_model: &str,
) -> Result<Vec<EmbeddingVector>, EmbeddingClientError> {
    if response
        .model
        .as_deref()
        .is_some_and(|model| model != expected_model)
    {
        return Err(EmbeddingClientError::InvalidResponse);
    }
    if response.data.len() != input_count {
        return Err(EmbeddingClientError::InvalidResponse);
    }
    let mut indexed: Vec<Option<EmbeddingVector>> =
        std::iter::repeat_with(|| None).take(input_count).collect();
    let mut dimensions = expected_dimensions;
    for item in response.data {
        if item.index >= input_count || indexed[item.index].is_some() {
            return Err(EmbeddingClientError::InvalidResponse);
        }
        if item.embedding.len() > MAX_EMBEDDING_DIMENSIONS {
            return Err(EmbeddingClientError::InvalidVector);
        }
        let values = item
            .embedding
            .into_iter()
            .map(|value| value as f32)
            .collect::<Vec<_>>();
        let vector = EmbeddingVector::new(values, dimensions)?;
        dimensions = Some(vector.dimensions());
        indexed[item.index] = Some(vector);
    }
    indexed
        .into_iter()
        .map(|vector| vector.ok_or(EmbeddingClientError::InvalidResponse))
        .collect()
}

#[cfg(test)]
mod tests {
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
}
