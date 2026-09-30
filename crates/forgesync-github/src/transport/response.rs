//! Bound and classify HTTP responses before provider DTO decoding.
//!
//! Successful JSON and error bodies have explicit size limits. The bounded prefix used for failure
//! classification avoids retaining arbitrary raw provider payloads. HTTP status, rate-limit hints,
//! redirects, and transport errors become typed failure categories for the retry layer.
//!
//! A request slot is acquired with cancellation before network I/O. The transport checks a
//! redirect target against the trusted origin rather than allowing the HTTP library to follow it
//! and possibly send authorization elsewhere.
//!
//! `ResponseBody` retains bounded bytes and trusted pagination, then consumes itself to decode the
//! successful payload. Invalid JSON is a terminal typed failure rather than a new request attempt.
//!
//! Resource modules consume the resulting typed value or error. They do not need to reason about
//! body stream limits, semaphore permits, or retryable network failures.

use reqwest::header::LOCATION;
use reqwest::{Response, StatusCode, Url};
use serde::de::DeserializeOwned;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::transport::retry::{api_failure_kind, body_identifies_rate_limit, retry_after_hint};
use crate::transport::{
    BodyReadError, GitHubResponse, MAX_ERROR_BODY_BYTES, RequestFailure, TrustedOrigin,
};

/// A bounded successful body and its already validated pagination destination.
///
/// The request traversal validates status, body size, and next-page origin before constructing this
/// value. Decoding consumes it so raw bytes do not escape alongside the typed provider payload.
pub struct ResponseBody {
    /// Current-page bytes collected within the successful-response size bound.
    pub body: Vec<u8>,
    /// Trusted next-page destination derived from response headers, independent of JSON content.
    pub next_page: Option<Url>,
}

impl ResponseBody {
    /// Decodes one successful page while retaining its validated pagination destination.
    ///
    /// Malformed JSON or a DTO shape mismatch returns `InvalidJson`; successful HTTP payload
    /// decoding is terminal and is not retried by the request loop. This conversion performs
    /// no additional network I/O and does not interpret provider family completeness.
    pub fn decode<T: DeserializeOwned>(self) -> Result<GitHubResponse<T>, GitHubError> {
        let value = serde_json::from_slice(&self.body).map_err(|_| GitHubError::InvalidJson)?;
        Ok(GitHubResponse {
            value,
            next_page: self.next_page,
        })
    }
}

/// Waits for a client permit or caller cancellation before sending a request.
pub async fn acquire_request_slot(
    slots: &std::sync::Arc<Semaphore>,
    cancellation: &CancellationToken,
) -> Result<OwnedSemaphorePermit, GitHubError> {
    tokio::select! {
        _ = cancellation.cancelled() => Err(GitHubError::Cancelled),
        permit = slots.clone().acquire_owned() => {
            permit.map_err(|_| GitHubError::ConcurrencyUnavailable)
        }
    }
}

/// Classifies a failed HTTP response after reading only a bounded error prefix. GitHub can use
/// HTTP 403 for both permission failures and rate limits, which have different retry behavior.
pub async fn classify_api_response(
    response: Response,
    status: StatusCode,
) -> Result<ResponseBody, RequestFailure> {
    let headers = response.headers().clone();
    let retry_after = retry_after_hint(&headers);
    let body = read_error_prefix(response).await.unwrap_or_default();
    let rate_limited = status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN
            && (headers
                .get("x-ratelimit-remaining")
                .and_then(|value| value.to_str().ok())
                == Some("0")
                || body_identifies_rate_limit(&body)));
    let kind = api_failure_kind(status, rate_limited);
    let error = GitHubError::Api {
        status: status.as_u16(),
        kind,
    };
    let retryable = status == StatusCode::TOO_MANY_REQUESTS
        || matches!(status.as_u16(), 500 | 502 | 503 | 504)
        || rate_limited;
    if retryable {
        Err(RequestFailure {
            error,
            retryable: true,
            retry_after,
        })
    } else {
        Err(RequestFailure::terminal(error))
    }
}

/// Retains only a bounded diagnostic prefix of a failed provider response.
pub async fn read_error_prefix(response: Response) -> Result<Vec<u8>, reqwest::Error> {
    read_body_prefix(response, MAX_ERROR_BODY_BYTES).await
}

/// Bounds successful response bodies even when `Content-Length` is absent or untrustworthy.
pub async fn read_body(response: Response, limit: usize) -> Result<Vec<u8>, BodyReadError> {
    let mut response = response;
    if response
        .content_length()
        .is_some_and(|length| length > u64::try_from(limit).unwrap_or(u64::MAX))
    {
        return Err(BodyReadError::TooLarge);
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(BodyReadError::Transport)? {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(BodyReadError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Reads a bounded prefix without exposing an unbounded provider payload.
pub async fn read_body_prefix(
    mut response: Response,
    limit: usize,
) -> Result<Vec<u8>, reqwest::Error> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let room = limit.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if bytes.len() == limit {
            break;
        }
    }
    Ok(bytes)
}

/// Resolves a redirect against the current URL and revalidates its trusted origin before the
/// client can forward credentials to the destination.
pub fn redirect_target(response: &Response, origin: &TrustedOrigin) -> Result<Url, GitHubError> {
    let location = response
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok());
    let Some(location) = location else {
        return Err(GitHubError::RedirectRejected);
    };
    let target = response
        .url()
        .join(location)
        .map_err(|_| GitHubError::RedirectRejected)?;
    origin.validate(&target)?;
    Ok(target)
}

/// Maps reqwest failures to retryable or terminal provider categories.
pub fn classify_transport_error(error: reqwest::Error) -> RequestFailure {
    if error.is_timeout() {
        RequestFailure::retryable(GitHubError::Timeout, None)
    } else {
        RequestFailure::retryable(GitHubError::Network, None)
    }
}

#[cfg(test)]
#[path = "response_tests.rs"]
mod tests;
