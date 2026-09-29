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

use super::request::ProviderRequest;
use super::{
    CancellationToken, DeserializeOwned, GitHubClient, GitHubClientConfig, GitHubError,
    GitHubResponse, GitHubToken, Method, Semaphore, TrustedOrigin, Url,
};

impl GitHubClient {
    /// Builds a reusable client that only sends requests to its configured API origin.
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

    /// Validates an API or pagination URL against the configured origin.
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

    /// Sends a GET request and deserializes a bounded JSON response.
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

    /// Sends a GET request and returns JSON content with a validated next-page URL.
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
