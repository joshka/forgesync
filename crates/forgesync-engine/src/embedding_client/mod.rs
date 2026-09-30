//! # External embedding-service adapter
//!
//! `EmbeddingClientConfig` describes the endpoint and model request settings; `EmbeddingClient`
//! sends document text and returns checked vectors. This is the network boundary for derived
//! embeddings, distinct from the GitHub source adapter.
//!
//! `response` validates shape, count, and dimensions before a vector can enter the archive.
//! `error` classifies service and validation failures for reports. Workflow batching and
//! persistence live in `embeddings`, so callers can reason separately about transport and document
//! selection.
//!
//! Clones share connection pooling and concurrency slots. A batch holds its slot through retries;
//! cancellation can interrupt both the queue and active work. The retry clock starts after slot
//! acquisition, so the configured budget does not bound time spent waiting behind another batch.
//! Archive leases belong to the embedding workflow, not this adapter.

mod error;
mod response;

use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use error::EmbeddingClientError;
use forgesync_core::embedding::EmbeddingVector;
use reqwest::Url;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue, USER_AGENT};
use serde::Serialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::embedding_client::response::EmbeddingResponse;

/// Bounds successful service payload memory independently of configured vector dimensions.
const MAX_RESPONSE_BODY_BYTES: usize = 16 * 1024 * 1024;
/// Hard upper bound for request input count, in addition to configured batch and byte limits.
const MAX_BATCH_INPUTS: usize = 2048;
/// Caps exponential backoff; the remaining total request budget may reject even this delay.
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
    /// Budget covering attempts and retry delays after acquiring a concurrency slot.
    /// Waiting for a slot is cancellable but excluded from this budget.
    pub total_budget: Duration,
    /// Maximum attempts for transient network, rate-limit, and server failures.
    pub max_attempts: u32,
}

/// OpenAI-compatible embeddings client with bounded requests and strict response validation.
#[derive(Clone)]
pub struct EmbeddingClient {
    /// Shared connection pool with per-request timeout and redirects disabled.
    http: reqwest::Client,
    /// Final POST URL, including the embeddings path appended to the configured base.
    endpoint: Url,
    /// Credential-free base URL recorded in vector recipes, distinct from the POST URL.
    endpoint_identity: String,
    /// Trimmed model identifier sent to the service and recorded with stored vectors.
    model: String,
    /// Shared secret used only to construct a sensitive authorization header.
    api_key: Arc<str>,
    /// Requested output width; response validation enforces it when configured.
    dimensions: Option<u32>,
    /// Individual input byte ceiling used by chunk selection and request validation.
    max_input_bytes: usize,
    /// Aggregate input byte ceiling used by batching and request validation.
    max_batch_input_bytes: usize,
    /// Configured input-count ceiling, additionally bounded by the adapter's hard limit.
    batch_size: usize,
    /// Configured parallelism advertised to the scheduler, not currently available permits.
    concurrency: usize,
    /// Attempt and backoff budget measured only after acquiring a shared slot.
    total_budget: Duration,
    /// Total allowed attempts, including the first request before any retry.
    max_attempts: u32,
    /// Slots shared across clones; each active batch retains one through retry waits.
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

    /// Budget for one batch's attempts and retry delays after acquiring its concurrency slot.
    ///
    /// Queue waiting is excluded. The embedding workflow uses this duration when sizing its writer
    /// lease, but this value does not bound the entire workflow or its lease-holding time.
    pub const fn request_budget(&self) -> Duration {
        self.total_budget
    }

    /// Requests one vector per input and validates every returned index and component.
    ///
    /// Empty input returns immediately. Other batches validate input limits before waiting for a
    /// shared concurrency slot; cancellation interrupts that wait. The retry budget starts once
    /// the slot is acquired and covers request execution, decoding, and backoff. The slot remains
    /// held until success, terminal failure, budget exhaustion, or cancellation.
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
                    self.wait_to_retry(attempt, started, cancellation).await?;
                }
                Err(error) => return Err(error),
            }
        }
        Err(EmbeddingClientError::RetryBudgetExhausted)
    }

    /// Waits only when backoff fits strictly inside the remaining batch budget.
    ///
    /// `attempt` is the zero-based attempt that just failed; the caller checks retryability and
    /// attempt limits before entering this wait. A delay equal to the remaining budget is rejected.
    /// Caller cancellation interrupts sleep while the enclosing batch retains its concurrency slot.
    async fn wait_to_retry(
        &self,
        attempt: u32,
        started: Instant,
        cancellation: &CancellationToken,
    ) -> Result<(), EmbeddingClientError> {
        let delay = retry_delay(attempt);
        let remaining = self.total_budget.saturating_sub(started.elapsed());
        if delay >= remaining {
            return Err(EmbeddingClientError::RetryBudgetExhausted);
        }
        tokio::select! {
            _ = cancellation.cancelled() => Err(EmbeddingClientError::Cancelled),
            _ = tokio::time::sleep(delay) => Ok(()),
        }
    }

    /// Limits simultaneous embedding calls before provider I/O begins.
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

    /// Sends one bounded service request and returns its validated response.
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
        response.validate(inputs.len(), self.dimensions, &self.model)
    }

    /// Rejects empty or oversized inputs before consuming a request slot.
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

/// Borrowed wire payload for one OpenAI-compatible embedding request.
///
/// Batch policy and input validation precede construction. This type describes protocol fields
/// only; credentials remain in a sensitive HTTP header and never enter the serialized JSON body.
#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    /// Configured provider model name, retained exactly in the request.
    model: &'a str,
    /// Validated text inputs whose positions determine returned vector ordering.
    input: &'a [String],
    /// Optional requested dimensions; absence omits the field for the provider default.
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<u32>,
    /// Explicit float encoding, matching the checked numeric response decoder.
    encoding_format: &'static str,
}

/// Checks endpoint and resource bounds before constructing the client.
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

/// Restricts embedding requests to supported endpoint URLs.
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

/// Identifies a local endpoint eligible for development configuration.
fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// Bounds the backoff applied after a retryable embedding failure.
fn retry_delay(attempt: u32) -> Duration {
    let exponent = attempt.min(5);
    Duration::from_millis(200_u64.saturating_mul(1_u64 << exponent)).min(MAX_RETRY_DELAY)
}

/// Classifies transport failures without exposing request content.
fn map_request_error(error: reqwest::Error) -> EmbeddingClientError {
    if error.is_timeout() {
        EmbeddingClientError::Timeout
    } else {
        EmbeddingClientError::Network
    }
}

/// Stops oversized responses before decoding provider data.
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

#[cfg(test)]
mod tests;
