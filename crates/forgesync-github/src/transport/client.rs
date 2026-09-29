//! Client transport behavior.

use tracing::Instrument;

use super::pagination::next_page_from_headers;
use super::response::{
    acquire_request_slot, classify_api_response, classify_transport_error, read_body,
    redirect_target,
};
use super::retry::retry_backoff;
use super::{
    ACCEPT, AUTHORIZATION, BodyReadError, CONTENT_TYPE, CancellationToken, DeserializeOwned,
    GitHubClient, GitHubClientConfig, GitHubError, GitHubResponse, GitHubToken, Instant,
    MAX_REDIRECTS, MAX_SUCCESS_BODY_BYTES, Method, OwnedSemaphorePermit, RequestFailure,
    ResponseBody, Semaphore, TrustedOrigin, USER_AGENT, Url,
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
        let start = Instant::now();

        for attempt in 1..=self.retry.max_attempts.get() {
            if cancellation.is_cancelled() {
                return Err(GitHubError::Cancelled);
            }
            let remaining = self.retry.total_budget.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(GitHubError::Deferred { retry_after: None });
            }

            let permit = tokio::select! {
                _ = cancellation.cancelled() => return Err(GitHubError::Cancelled),
                result = tokio::time::timeout(
                    remaining,
                    acquire_request_slot(&self.request_slots, cancellation),
                ) => match result {
                    Ok(permit) => permit?,
                    Err(_) => return Err(GitHubError::Deferred { retry_after: None }),
                },
            };
            let span = tracing::debug_span!(
                "github_http_request",
                origin = %self.origin.display,
                method = %method,
                attempt,
            );
            let request = self.perform_once(url, &method, body, permit, cancellation);
            let outcome = tokio::select! {
                _ = cancellation.cancelled() => return Err(GitHubError::Cancelled),
                result = tokio::time::timeout(remaining, request).instrument(span) => {
                    match result {
                        Ok(outcome) => outcome,
                        Err(_) => return Err(GitHubError::Deferred { retry_after: None }),
                    }
                }
            };

            match outcome {
                Ok(response) => {
                    let value = serde_json::from_slice(&response.body)
                        .map_err(|_| GitHubError::InvalidJson)?;
                    return Ok(GitHubResponse {
                        value,
                        next_page: response.next_page,
                    });
                }
                Err(failure) if failure.retryable => {
                    let delay = failure
                        .retry_after
                        .unwrap_or_else(|| retry_backoff(&self.retry, attempt.saturating_sub(1)));
                    let remaining = self.retry.total_budget.saturating_sub(start.elapsed());
                    if delay >= remaining {
                        return Err(GitHubError::Deferred {
                            retry_after: failure.retry_after,
                        });
                    }
                    if attempt == self.retry.max_attempts.get() {
                        if failure.retry_after.is_some_and(|wait| !wait.is_zero()) {
                            return Err(GitHubError::Deferred {
                                retry_after: failure.retry_after,
                            });
                        }
                        return Err(failure.error);
                    }
                    tokio::select! {
                        _ = cancellation.cancelled() => return Err(GitHubError::Cancelled),
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
                Err(failure) => return Err(failure.error),
            }
        }

        Err(GitHubError::Deferred { retry_after: None })
    }

    async fn perform_once(
        &self,
        url: &Url,
        method: &Method,
        body: Option<&[u8]>,
        _permit: OwnedSemaphorePermit,
        cancellation: &CancellationToken,
    ) -> Result<ResponseBody, RequestFailure> {
        let mut current_url = url.clone();
        let mut visited = std::collections::HashSet::new();
        for redirect_count in 0..=MAX_REDIRECTS {
            if !visited.insert(current_url.as_str().to_owned()) {
                return Err(RequestFailure::terminal(GitHubError::RedirectRejected));
            }
            self.origin
                .validate(&current_url)
                .map_err(RequestFailure::terminal)?;
            let mut request = self
                .http
                .request(method.clone(), current_url.clone())
                .header(ACCEPT, "application/vnd.github+json")
                .header(USER_AGENT, "forgesync");
            if let Some(body) = body {
                request = request
                    .header(CONTENT_TYPE, "application/json")
                    .body(body.to_vec());
            }
            if let Some(token) = &self.token {
                request = request.header(AUTHORIZATION, format!("Bearer {}", token.expose()));
            }

            let response = tokio::select! {
                _ = cancellation.cancelled() => return Err(RequestFailure::terminal(GitHubError::Cancelled)),
                response = request.send() => response.map_err(classify_transport_error)?,
            };
            let status = response.status();
            tracing::debug!(
                status = status.as_u16(),
                redirect_count,
                "GitHub response received"
            );

            if status.is_redirection() {
                if redirect_count == MAX_REDIRECTS {
                    return Err(RequestFailure::terminal(GitHubError::RedirectRejected));
                }
                current_url =
                    redirect_target(&response, &self.origin).map_err(RequestFailure::terminal)?;
                continue;
            }
            if !status.is_success() {
                return classify_api_response(response, status).await;
            }

            let next_page =
                next_page_from_headers(response.url(), response.headers(), &self.origin)
                    .map_err(RequestFailure::terminal)?;
            let body = read_body(response, MAX_SUCCESS_BODY_BYTES)
                .await
                .map_err(|error| match error {
                    BodyReadError::TooLarge => {
                        RequestFailure::terminal(GitHubError::ResponseTooLarge)
                    }
                    BodyReadError::Transport(error) => classify_transport_error(error),
                })?;
            return Ok(ResponseBody { body, next_page });
        }

        Err(RequestFailure::terminal(GitHubError::RedirectRejected))
    }
}
