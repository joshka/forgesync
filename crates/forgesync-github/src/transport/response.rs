//! Bound and classify HTTP responses before provider DTO decoding.
//!
//! Successful and error bodies have explicit size limits; only a bounded error prefix is read for
//! rate-limit classification, so arbitrary provider payloads are never retained.

use std::error::Error as _;

use reqwest::{Response, StatusCode, Url};
use serde::de::DeserializeOwned;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::transport::retry::{api_failure_kind, body_identifies_rate_limit, retry_after_hint};
use crate::transport::{BodyReadError, GitHubResponse, MAX_ERROR_BODY_BYTES, RequestFailure};

/// A bounded successful body and its next-page link.
pub struct ResponseBody {
    pub body: Vec<u8>,
    pub next_page: Option<Url>,
}

impl ResponseBody {
    /// Decodes one successful page. Invalid JSON or a DTO shape mismatch is terminal `InvalidJson`.
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
            Ok(permit.expect("the client never closes its request semaphore"))
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
    let body = read_body_prefix(response, MAX_ERROR_BODY_BYTES)
        .await
        .unwrap_or_default();
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
        Err(RequestFailure::retryable(error, retry_after))
    } else {
        Err(RequestFailure::terminal(error))
    }
}

/// Bounds successful response bodies even when `Content-Length` is absent or untrustworthy.
pub async fn read_body(mut response: Response, limit: usize) -> Result<Vec<u8>, BodyReadError> {
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

async fn read_body_prefix(mut response: Response, limit: usize) -> Result<Vec<u8>, reqwest::Error> {
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

/// Maps reqwest failures to retryable or terminal provider categories.
///
/// A redirect stopped by the client's policy carries the [`GitHubError`] it chose.
pub fn classify_transport_error(error: reqwest::Error) -> RequestFailure {
    if error.is_redirect() {
        let stopped = error
            .source()
            .and_then(|source| source.downcast_ref::<GitHubError>())
            .cloned()
            .unwrap_or(GitHubError::RedirectRejected);
        RequestFailure::terminal(stopped)
    } else if error.is_timeout() {
        RequestFailure::retryable(GitHubError::Timeout, None)
    } else {
        RequestFailure::retryable(GitHubError::Network, None)
    }
}

#[cfg(test)]
#[path = "response_tests.rs"]
mod tests;
