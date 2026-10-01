//! Budgeted request loop: permits, attempts, retry waits, and bounded body reading.

use std::time::Instant;

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::{Method, Url};
use serde::de::DeserializeOwned;
use tokio::sync::OwnedSemaphorePermit;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::error::GitHubError;
use crate::transport::pagination::next_page_from_headers;
use crate::transport::response::{
    ResponseBody, acquire_request_slot, classify_api_response, classify_transport_error, read_body,
};
use crate::transport::retry::retry_backoff;
use crate::transport::{
    BodyReadError, GitHubClient, GitHubResponse, MAX_SUCCESS_BODY_BYTES, RequestFailure,
};

/// Immutable request data shared by all budgeted attempts.
pub struct ProviderRequest<'a> {
    pub client: &'a GitHubClient,
    /// Destination already validated against the client origin.
    pub url: &'a Url,
    pub method: Method,
    /// Encoded JSON body, borrowed for repeated attempts.
    pub body: Option<&'a [u8]>,
    pub cancellation: &'a CancellationToken,
}

impl ProviderRequest<'_> {
    /// Retries within the configured budget and decodes the first successful response.
    pub async fn run<T: DeserializeOwned>(&self) -> Result<GitHubResponse<T>, GitHubError> {
        let start = Instant::now();

        for attempt in 1..=self.client.retry.max_attempts.get() {
            if self.cancellation.is_cancelled() {
                return Err(GitHubError::Cancelled);
            }
            let remaining = self
                .client
                .retry
                .total_budget
                .saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(GitHubError::Deferred { retry_after: None });
            }

            match self.attempt(remaining, attempt).await? {
                Ok(response) => return response.decode(),
                Err(failure) if failure.retryable => {
                    self.wait_to_retry(failure, attempt, start).await?;
                }
                Err(failure) => return Err(failure.error),
            }
        }

        Err(GitHubError::Deferred { retry_after: None })
    }

    /// Acquires a slot and bounds a single attempt within the remaining budget.
    async fn attempt(
        &self,
        remaining: std::time::Duration,
        attempt: u32,
    ) -> Result<Result<ResponseBody, RequestFailure>, GitHubError> {
        let permit = tokio::select! {
            _ = self.cancellation.cancelled() => return Err(GitHubError::Cancelled),
            result = tokio::time::timeout(
                remaining,
                acquire_request_slot(&self.client.request_slots, self.cancellation),
            ) => match result {
                Ok(permit) => permit?,
                Err(_) => return Err(GitHubError::Deferred { retry_after: None }),
            },
        };
        let span = tracing::debug_span!(
            "github_http_request",
            origin = %self.client.origin.display,
            method = %self.method,
            attempt,
        );
        let request = self.perform_once(permit);
        let outcome = tokio::select! {
            _ = self.cancellation.cancelled() => return Err(GitHubError::Cancelled),
            result = tokio::time::timeout(remaining, request).instrument(span) => {
                match result {
                    Ok(outcome) => outcome,
                    Err(_) => return Err(GitHubError::Deferred { retry_after: None }),
                }
            }
        };

        Ok(outcome)
    }

    /// Honors provider backoff without exceeding the request budget or retry count.
    async fn wait_to_retry(
        &self,
        failure: RequestFailure,
        attempt: u32,
        start: Instant,
    ) -> Result<(), GitHubError> {
        let delay = failure
            .retry_after
            .unwrap_or_else(|| retry_backoff(&self.client.retry, attempt.saturating_sub(1)));
        let remaining = self
            .client
            .retry
            .total_budget
            .saturating_sub(start.elapsed());
        if delay >= remaining {
            return Err(GitHubError::Deferred {
                retry_after: failure.retry_after,
            });
        }
        if attempt == self.client.retry.max_attempts.get() {
            if failure.retry_after.is_some_and(|wait| !wait.is_zero()) {
                return Err(GitHubError::Deferred {
                    retry_after: failure.retry_after,
                });
            }
            return Err(failure.error);
        }
        tokio::select! {
            _ = self.cancellation.cancelled() => return Err(GitHubError::Cancelled),
            _ = tokio::time::sleep(delay) => {}
        }
        Ok(())
    }

    /// Holds the permit through the request, client-followed redirects, and body reading.
    async fn perform_once(
        &self,
        _permit: OwnedSemaphorePermit,
    ) -> Result<ResponseBody, RequestFailure> {
        let response = self.send().await?;
        let status = response.status();
        tracing::debug!(status = status.as_u16(), "GitHub response received");

        // The redirect policy follows trusted redirects; one left here had no usable Location.
        if status.is_redirection() {
            return Err(RequestFailure::terminal(GitHubError::RedirectRejected));
        }
        if !status.is_success() {
            return classify_api_response(response, status).await;
        }
        self.read_success(response).await
    }

    async fn send(&self) -> Result<reqwest::Response, RequestFailure> {
        let mut request = self
            .client
            .http
            .request(self.method.clone(), self.url.clone())
            .header(ACCEPT, "application/vnd.github+json")
            .header(USER_AGENT, "forgesync");
        if let Some(body) = self.body {
            request = request
                .header(CONTENT_TYPE, "application/json")
                .body(body.to_vec());
        }
        if let Some(token) = &self.client.token {
            request = request.header(AUTHORIZATION, format!("Bearer {}", token.expose()));
        }

        tokio::select! {
            _ = self.cancellation.cancelled() => Err(RequestFailure::terminal(GitHubError::Cancelled)),
            response = request.send() => response.map_err(classify_transport_error),
        }
    }

    /// Parses pagination metadata and reads a bounded successful response body.
    async fn read_success(
        &self,
        response: reqwest::Response,
    ) -> Result<ResponseBody, RequestFailure> {
        let next_page = next_page_from_headers(response.url(), response.headers())
            .map_err(RequestFailure::terminal)?;
        let body =
            read_body(response, MAX_SUCCESS_BODY_BYTES)
                .await
                .map_err(|error| match error {
                    BodyReadError::TooLarge => {
                        RequestFailure::terminal(GitHubError::ResponseTooLarge)
                    }
                    BodyReadError::Transport(error) => classify_transport_error(error),
                })?;
        Ok(ResponseBody { body, next_page })
    }
}
