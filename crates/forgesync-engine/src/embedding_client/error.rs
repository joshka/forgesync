//! Sanitized embedding-service failures.
//!
//! No variant contains credentials, input text, raw bodies, or a transport source chain. Retry
//! covers network/timeout failures, 429, and server errors within the client's budgets.

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
    /// The caller supplied no usable API key; the library performs no environment lookup.
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
    /// Returns the stable reporting category without private request/response details.
    ///
    /// All HTTP statuses share one category; the typed variant retains the numeric status.
    /// This code is not a retry or fallback decision. Human display text is a separate contract.
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

    /// Classifies transient attempt failures for the adapter's bounded retry loop.
    ///
    /// Network/timeouts, 429, and statuses at least 500 are eligible. The caller still enforces
    /// attempt/time budgets. Restricted visibility keeps this adapter policy off the public error
    /// API even though the error type is re-exported for reporting.
    pub(super) fn retryable(self) -> bool {
        match self {
            Self::Network | Self::Timeout => true,
            Self::ApiStatus(status) => status == 429 || status >= 500,
            _ => false,
        }
    }
}

impl From<EmbeddingVectorError> for EmbeddingClientError {
    /// Maps domain vector-validation failures to the service's stable invalid-vector category.
    fn from(_: EmbeddingVectorError) -> Self {
        Self::InvalidVector
    }
}

#[cfg(test)]
mod tests {
    //! Retry eligibility is separate from reporting and fallback.
    //!
    //! Named inputs establish the bounded loop's classification without making HTTP requests.
    //! Client scenarios cover actual attempts and cancellation; these cases keep policy explicit.

    use crate::embedding_client::EmbeddingClientError;

    #[rstest::rstest]
    #[case::network(EmbeddingClientError::Network)]
    #[case::timeout(EmbeddingClientError::Timeout)]
    #[case::rate_limit(EmbeddingClientError::ApiStatus(429))]
    #[case::server_error(EmbeddingClientError::ApiStatus(500))]
    fn transient_failure_is_retry_eligible(#[case] error: EmbeddingClientError) {
        assert!(error.retryable());
    }

    #[rstest::rstest]
    #[case::unauthorized(EmbeddingClientError::ApiStatus(401))]
    #[case::cancelled(EmbeddingClientError::Cancelled)]
    #[case::invalid_response(EmbeddingClientError::InvalidResponse)]
    #[case::redirect(EmbeddingClientError::RedirectRejected)]
    fn permanent_or_cancelled_failure_is_not_retried(#[case] error: EmbeddingClientError) {
        assert!(!error.retryable());
    }
}
