//! Execute bounded provider requests through one trusted [`crate::transport::GitHubClient`].
//!
//! Construction checks configuration and creates an HTTP client with automatic redirects disabled.
//! GET and POST methods validate destinations, acquire concurrency permits, and decode bounded
//! JSON. `perform_once` handles one attempt; retry policy in the parent module decides whether and
//! when to repeat it.
//!
//! This layer owns request I/O, not provider resource meaning. `resources` and `review_threads`
//! choose endpoints and deserialize typed DTOs. Responses carry a validated next-page URL when
//! available; the caller still decides whether the complete resource family has been acquired.
//!
//! Keep authorization attached to the configured origin. A redirect or pagination link to another
//! origin must fail before it can receive the token, even if the link came from GitHub's response.

use reqwest::{Method, Url};
use serde::de::DeserializeOwned;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::token::GitHubToken;
use crate::transport::request::ProviderRequest;
use crate::transport::{GitHubClient, GitHubClientConfig, GitHubResponse, TrustedOrigin};

impl GitHubClient {
    /// Builds a reusable transport restricted to its configured API origin.
    ///
    /// Construction checks timeouts, retry bounds, and the API base URL, but sends no request and
    /// discovers no credentials. Client clones share the HTTP pool and concurrency permits.
    /// Automatic redirects are disabled; request code validates each redirected destination before
    /// adding authorization. Resource acquisition still needs explicit caller cancellation.
    ///
    /// # Errors
    ///
    /// Returns typed configuration/base-URL errors for unusable settings and
    /// [`GitHubError::ClientInitialization`] when the HTTP client cannot be built.
    pub fn new(
        config: GitHubClientConfig,
        token: Option<GitHubToken>,
    ) -> Result<Self, GitHubError> {
        if config.request_timeout.is_zero()
            || config.retry.total_budget.is_zero()
            || config.retry.max_backoff < config.retry.initial_backoff
        {
            return Err(GitHubError::InvalidConfiguration);
        }
        let origin = TrustedOrigin::parse(&config.api_base_url)?;
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("forgesync")
            .build()
            .map_err(|_| GitHubError::ClientInitialization)?;

        Ok(Self {
            http,
            api_base_url: config.api_base_url,
            origin,
            token,
            retry: config.retry,
            request_slots: std::sync::Arc::new(Semaphore::new(config.max_in_flight.get())),
        })
    }

    /// Checks a destination against the configured scheme, host, and effective port.
    ///
    /// Validation sends no request and does not establish resource existence or permission. A
    /// mismatched origin or credential-bearing candidate returns [`GitHubError::UntrustedOrigin`].
    pub fn validate_destination(&self, candidate: &Url) -> Result<(), GitHubError> {
        self.origin.validate(candidate)
    }

    /// Resolves and validates a relative or absolute pagination link.
    pub fn resolve_pagination_url(
        &self,
        current_url: &Url,
        link: &str,
    ) -> Result<Url, GitHubError> {
        self.origin.validate(current_url)?;
        let candidate = current_url
            .join(link)
            .map_err(|_| GitHubError::UntrustedOrigin)?;
        self.origin.validate(&candidate)?;
        Ok(candidate)
    }

    /// Creates a URL by appending encoded path segments to the configured API base path.
    pub fn endpoint_url(&self, path_segments: &[&str]) -> Result<Url, GitHubError> {
        let mut url = self.api_base_url.clone();
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| GitHubError::InvalidApiBaseUrl)?;
        segments.pop_if_empty();
        for segment in path_segments {
            if segment.is_empty() {
                return Err(GitHubError::InvalidApiBaseUrl);
            }
            segments.push(segment);
        }
        drop(segments);
        Ok(url)
    }

    /// Acquires one bounded JSON page and returns its decoded content without continuation
    /// metadata.
    ///
    /// Use [`Self::get_json_page`] when resource completeness requires following REST pagination.
    /// This method uses the same origin validation, permit, retry, and cancellation policy, and
    /// returns the same typed transport/API/decoding failures.
    pub async fn get_json<T>(
        &self,
        url: &Url,
        cancellation: &CancellationToken,
    ) -> Result<T, GitHubError>
    where
        T: DeserializeOwned,
    {
        Ok(self.get_json_page(url, cancellation).await?.value)
    }

    /// Acquires one bounded JSON page with validated REST continuation metadata.
    ///
    /// Waits for a shared request permit, follows only validated bounded redirects, and applies the
    /// configured request retry budget. Caller cancellation can stop permit waits, attempts, or
    /// retry delays. A successful body is bounded before decoding, and any next-page URL is checked
    /// before being returned. This method does not follow that continuation or claim a complete
    /// evidence-family collection.
    ///
    /// # Errors
    ///
    /// Returns typed destination, cancellation, request-budget, transport, API-status, body-limit,
    /// pagination, and JSON-decoding failures. Safe errors omit credentials and raw response
    /// bodies.
    pub async fn get_json_page<T>(
        &self,
        url: &Url,
        cancellation: &CancellationToken,
    ) -> Result<GitHubResponse<T>, GitHubError>
    where
        T: DeserializeOwned,
    {
        self.request_json(url, cancellation).await
    }

    /// Sends a bounded JSON POST request to the configured API origin.
    pub(crate) async fn post_json<T>(
        &self,
        url: &Url,
        body: &[u8],
        cancellation: &CancellationToken,
    ) -> Result<T, GitHubError>
    where
        T: DeserializeOwned,
    {
        Ok(self
            .request_json_with_body(url, Method::POST, Some(body), cancellation)
            .await?
            .value)
    }

    /// Builds GitHub's GraphQL endpoint from the configured REST API base URL.
    pub(crate) fn graphql_endpoint_url(&self) -> Result<Url, GitHubError> {
        let mut url = self.api_base_url.clone();
        let path = url.path().trim_end_matches('/');
        let api_path = path.strip_suffix("/v3").unwrap_or(path);
        url.set_path(&format!("{api_path}/graphql"));
        self.origin.validate(&url)?;
        Ok(url)
    }

    /// Sends a JSON request with retries and validated pagination metadata.
    async fn request_json<T>(
        &self,
        url: &Url,
        cancellation: &CancellationToken,
    ) -> Result<GitHubResponse<T>, GitHubError>
    where
        T: DeserializeOwned,
    {
        self.request_json_with_body(url, Method::GET, None, cancellation)
            .await
    }

    /// Sends a bounded JSON body while enforcing the trusted API origin.
    async fn request_json_with_body<T>(
        &self,
        url: &Url,
        method: Method,
        body: Option<&[u8]>,
        cancellation: &CancellationToken,
    ) -> Result<GitHubResponse<T>, GitHubError>
    where
        T: DeserializeOwned,
    {
        self.origin.validate(url)?;
        let request = ProviderRequest {
            client: self,
            url,
            method,
            body,
            cancellation,
        };
        request.run().await
    }
}
