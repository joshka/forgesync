//! # Typed embedding-service failures
//!
//! `EmbeddingClientError` distinguishes request, response, and vector-validation problems. The
//! workflow uses this information to report failed batches without treating malformed service
//! output as a valid empty embedding.
//!
//! This error belongs at the adapter boundary. Engine-level reports can classify it, while the
//! client keeps protocol details out of archive and search modules.

use forgesync_core::embedding::EmbeddingVectorError;
use thiserror::Error;

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

    /// Identifies service failures eligible for the bounded retry policy.
    pub(super) fn retryable(self) -> bool {
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
