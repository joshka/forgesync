//! Client construction and the JSON request entry points.

use reqwest::{Method, Url};
use serde::de::DeserializeOwned;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::token::GitHubToken;
use crate::transport::request::ProviderRequest;
use crate::transport::{
    GitHubClient, GitHubClientConfig, GitHubResponse, MAX_REDIRECTS, TrustedOrigin,
};

impl GitHubClient {
    /// Builds a reusable transport restricted to its configured API origin.
    ///
    /// Construction sends no request. Client clones share the HTTP pool and concurrency permits.
    /// Redirects are followed only within the trusted origin so authorization never leaves it.
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
        let redirect_origin = origin.clone();
        let redirect = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() > MAX_REDIRECTS {
                attempt.error(GitHubError::RedirectRejected)
            } else if let Err(error) = redirect_origin.validate(attempt.url()) {
                attempt.error(error)
            } else {
                attempt.follow()
            }
        });
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .redirect(redirect)
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

    /// Acquires one bounded JSON page and its REST next-page link.
    ///
    /// `url` may be a caller-supplied continuation; it must share the configured origin. The
    /// returned next page is not followed.
    ///
    /// # Errors
    ///
    /// Returns typed destination, cancellation, request-budget, transport, API-status, body-limit,
    /// pagination, and JSON-decoding failures. Errors omit credentials and response bodies.
    pub async fn get_json_page<T>(
        &self,
        url: &Url,
        cancellation: &CancellationToken,
    ) -> Result<GitHubResponse<T>, GitHubError>
    where
        T: DeserializeOwned,
    {
        self.request_json(url, Method::GET, None, cancellation)
            .await
    }

    /// Sends a JSON POST; crate-only so callers use typed GraphQL operations.
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
            .request_json(url, Method::POST, Some(body), cancellation)
            .await?
            .value)
    }

    /// Builds GitHub's GraphQL endpoint from the configured REST API base URL.
    ///
    /// Enterprise REST bases end in `/api/v3`; their GraphQL endpoint is `/api/graphql`.
    pub(crate) fn graphql_endpoint_url(&self) -> Url {
        let mut url = self.api_base_url.clone();
        let path = url.path().trim_end_matches('/');
        let api_path = path.strip_suffix("/v3").unwrap_or(path);
        url.set_path(&format!("{api_path}/graphql"));
        url
    }

    /// Validates the destination once, then runs the budgeted request.
    async fn request_json<T>(
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
