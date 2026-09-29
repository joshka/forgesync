//! HTTP client configuration, retries, redirects, and pagination safety.
//!
//! Construct a client from explicit configuration and a supplied token. Transport owns HTTP,
//! retries, and response classification; it does not open archives or discover process credentials.

use std::num::{NonZeroU32, NonZeroUsize};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::header::{
    ACCEPT, AUTHORIZATION, CONTENT_TYPE, LINK, LOCATION, RETRY_AFTER, USER_AGENT,
};
use reqwest::{Method, Response, StatusCode, Url};
use serde::de::DeserializeOwned;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::error::{ApiFailureKind, GitHubError};
use crate::token::GitHubToken;

const MAX_SUCCESS_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;
const MAX_REDIRECTS: usize = 5;

/// Retry limits for transient network, server, and rate-limit failures.
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    /// Maximum requests, including the first attempt.
    pub max_attempts: NonZeroU32,
    /// Total time allowed for the initial request and automatic retries.
    pub total_budget: Duration,
    /// Delay before the first retry when the provider supplied no wait hint.
    pub initial_backoff: Duration,
    /// Maximum locally selected delay between retries.
    pub max_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: NonZeroU32::new(5).expect("five is non-zero"),
            total_budget: Duration::from_secs(120),
            initial_backoff: Duration::from_millis(250),
            max_backoff: Duration::from_secs(10),
        }
    }
}

/// Configuration for the shared GitHub API client.
#[derive(Clone, Debug)]
pub struct GitHubClientConfig {
    /// REST or GraphQL API base URL, such as `https://api.github.com/`.
    pub api_base_url: Url,
    /// Timeout for one HTTP request and its response body.
    pub request_timeout: Duration,
    /// Maximum number of requests in flight at once.
    pub max_in_flight: NonZeroUsize,
    /// Retry count, wait budget, and backoff behavior.
    pub retry: RetryPolicy,
}

impl GitHubClientConfig {
    /// Creates configuration for a GitHub API base URL with conservative defaults.
    pub fn new(api_base_url: Url) -> Self {
        Self {
            api_base_url,
            request_timeout: Duration::from_secs(30),
            max_in_flight: NonZeroUsize::new(4).expect("four is non-zero"),
            retry: RetryPolicy::default(),
        }
    }
}

/// Shared read-only client for GitHub REST or GraphQL JSON endpoints.
#[derive(Clone)]
pub struct GitHubClient {
    http: reqwest::Client,
    api_base_url: Url,
    origin: TrustedOrigin,
    token: Option<GitHubToken>,
    retry: RetryPolicy,
    request_slots: std::sync::Arc<Semaphore>,
}

/// JSON content and the validated next URL from a GitHub REST Link header.
#[derive(Clone, Debug)]
pub struct GitHubResponse<T> {
    /// Deserialized current-page content.
    pub value: T,
    /// Next page URL, when the provider reports one.
    pub next_page: Option<Url>,
}

mod client;
mod pagination;
mod response;
mod retry;

struct RequestFailure {
    error: GitHubError,
    retryable: bool,
    retry_after: Option<Duration>,
}

struct ResponseBody {
    body: Vec<u8>,
    next_page: Option<Url>,
}

impl RequestFailure {
    fn terminal(error: GitHubError) -> Self {
        Self {
            error,
            retryable: false,
            retry_after: None,
        }
    }

    fn retryable(error: GitHubError, retry_after: Option<Duration>) -> Self {
        Self {
            error,
            retryable: true,
            retry_after,
        }
    }
}

enum BodyReadError {
    TooLarge,
    Transport(reqwest::Error),
}

#[derive(Clone)]
struct TrustedOrigin {
    origin: url::Origin,
    display: String,
}

impl TrustedOrigin {
    fn parse(base_url: &Url) -> Result<Self, GitHubError> {
        if !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
            || base_url.host_str().is_none()
            || !is_allowed_scheme(base_url)
        {
            return Err(GitHubError::InvalidApiBaseUrl);
        }
        let origin = base_url.origin();
        let display = match base_url.host_str() {
            Some(host) => match base_url.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_owned(),
            },
            None => return Err(GitHubError::InvalidApiBaseUrl),
        };
        Ok(Self { origin, display })
    }

    fn validate(&self, candidate: &Url) -> Result<(), GitHubError> {
        if candidate.username().is_empty()
            && candidate.password().is_none()
            && candidate.origin() == self.origin
        {
            Ok(())
        } else {
            Err(GitHubError::UntrustedOrigin)
        }
    }
}

fn is_allowed_scheme(url: &Url) -> bool {
    if url.scheme() == "https" {
        return true;
    }
    if url.scheme() != "http" {
        return false;
    }
    match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests;
