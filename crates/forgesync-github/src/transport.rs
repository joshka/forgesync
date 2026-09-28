use std::num::{NonZeroU32, NonZeroUsize};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::header::{
    ACCEPT, AUTHORIZATION, CONTENT_TYPE, LINK, LOCATION, RETRY_AFTER, USER_AGENT,
};
use reqwest::{Method, Response, StatusCode, Url};
use serde::de::DeserializeOwned;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::{ApiFailureKind, GitHubError, GitHubToken};

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

async fn acquire_request_slot(
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

async fn classify_api_response(
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

async fn read_error_prefix(response: Response) -> Result<Vec<u8>, reqwest::Error> {
    read_body_prefix(response, MAX_ERROR_BODY_BYTES).await
}

async fn read_body(response: Response, limit: usize) -> Result<Vec<u8>, BodyReadError> {
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

fn redirect_target(response: &Response, origin: &TrustedOrigin) -> Result<Url, GitHubError> {
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

fn classify_transport_error(error: reqwest::Error) -> RequestFailure {
    if error.is_timeout() {
        RequestFailure::retryable(GitHubError::Timeout, None)
    } else {
        RequestFailure::retryable(GitHubError::Network, None)
    }
}

fn next_page_from_headers(
    current_url: &Url,
    headers: &reqwest::header::HeaderMap,
    origin: &TrustedOrigin,
) -> Result<Option<Url>, GitHubError> {
    for value in headers.get_all(LINK).iter() {
        let value = value
            .to_str()
            .map_err(|_| GitHubError::InvalidPaginationLink)?;
        for item in split_link_header(value) {
            if let Some(target) = next_link_target(item)? {
                let next = current_url
                    .join(target)
                    .map_err(|_| GitHubError::InvalidPaginationLink)?;
                origin.validate(&next)?;
                return Ok(Some(next));
            }
        }
    }
    Ok(None)
}

fn split_link_header(value: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut start = 0;
    let mut in_angle = false;
    let mut in_quotes = false;
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if in_quotes {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_quotes = false;
            }
            continue;
        }
        match character {
            '<' => in_angle = true,
            '>' => in_angle = false,
            '"' => in_quotes = true,
            ',' if !in_angle => {
                items.push(value[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    if start <= value.len() {
        items.push(value[start..].trim());
    }
    items
}

fn next_link_target(item: &str) -> Result<Option<&str>, GitHubError> {
    let Some(close_bracket) = item.find('>') else {
        if item.to_ascii_lowercase().contains("rel=\"next\"") {
            return Err(GitHubError::InvalidPaginationLink);
        }
        return Ok(None);
    };
    let Some(target) = item.strip_prefix('<') else {
        return Err(GitHubError::InvalidPaginationLink);
    };
    let target = &target[..close_bracket - 1];
    let parameters = &item[close_bracket + 1..];
    let is_next = parameters.split(';').any(|parameter| {
        let Some((name, value)) = parameter.trim().split_once('=') else {
            return false;
        };
        if !name.trim().eq_ignore_ascii_case("rel") {
            return false;
        }
        value
            .trim()
            .trim_matches('"')
            .split_ascii_whitespace()
            .any(|relation| relation.eq_ignore_ascii_case("next"))
    });
    if is_next && target.is_empty() {
        return Err(GitHubError::InvalidPaginationLink);
    }
    Ok(is_next.then_some(target))
}

fn api_failure_kind(status: StatusCode, rate_limited: bool) -> ApiFailureKind {
    if rate_limited {
        return ApiFailureKind::RateLimited;
    }
    match status {
        StatusCode::UNAUTHORIZED => ApiFailureKind::AuthenticationRequired,
        StatusCode::FORBIDDEN => ApiFailureKind::PermissionDenied,
        StatusCode::NOT_FOUND => ApiFailureKind::NotFound,
        StatusCode::CONFLICT => ApiFailureKind::Conflict,
        status if status.is_server_error() => ApiFailureKind::Server,
        _ => ApiFailureKind::Client,
    }
}

fn body_identifies_rate_limit(body: &[u8]) -> bool {
    let body = String::from_utf8_lossy(body).to_ascii_lowercase();
    [
        "api rate limit exceeded",
        "secondary rate limit",
        "abuse detection",
    ]
    .iter()
    .any(|marker| body.contains(marker))
}

fn retry_after_hint(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    if let Some(value) = headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
    {
        if let Ok(seconds) = value.trim().parse::<u64>() {
            return Some(Duration::from_secs(seconds));
        }
        if let Ok(retry_at) = httpdate::parse_http_date(value) {
            return Some(
                retry_at
                    .duration_since(SystemTime::now())
                    .unwrap_or(Duration::ZERO),
            );
        }
    }

    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok());
    if remaining != Some("0") {
        return None;
    }
    let reset = headers
        .get("x-ratelimit-reset")
        .and_then(|value| value.to_str().ok())?
        .parse::<u64>()
        .ok()?;
    let retry_at = UNIX_EPOCH.checked_add(Duration::from_secs(reset))?;
    Some(
        retry_at
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO),
    )
}

fn retry_backoff(policy: &RetryPolicy, retry_index: u32) -> Duration {
    let multiplier = 1_u32.checked_shl(retry_index.min(31)).unwrap_or(u32::MAX);
    policy
        .initial_backoff
        .saturating_mul(multiplier)
        .min(policy.max_backoff)
}

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
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use reqwest::Url;
    use serde::Deserialize;
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

    use crate::{ApiFailureKind, GitHubClient, GitHubClientConfig, GitHubError, GitHubToken};

    use super::{GitHubClientConfig as ClientConfig, RetryPolicy};

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct Message {
        message: String,
    }

    struct RetryThenSuccess(Arc<AtomicUsize>);

    impl Respond for RetryThenSuccess {
        fn respond(&self, _request: &Request) -> ResponseTemplate {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(503).set_body_json(serde_json::json!({"message": "retry"}))
            } else {
                ResponseTemplate::new(200)
                    .insert_header("Link", "<?page=2>; title=\"next, page\"; rel=\"next\"")
                    .set_body_json(serde_json::json!({"message": "ok"}))
            }
        }
    }

    fn config(server: &MockServer) -> GitHubClientConfig {
        let mut config = ClientConfig::new(Url::parse(&format!("{}/", server.uri())).unwrap());
        config.retry = RetryPolicy {
            max_attempts: std::num::NonZeroU32::new(3).unwrap(),
            total_budget: std::time::Duration::from_secs(2),
            initial_backoff: std::time::Duration::ZERO,
            max_backoff: std::time::Duration::ZERO,
        };
        config
    }

    #[tokio::test]
    async fn retries_selected_server_failures_and_sends_the_bearer_token() {
        let server = MockServer::start().await;
        let attempts = Arc::new(AtomicUsize::new(0));
        Mock::given(method("GET"))
            .and(path("/thread"))
            .and(header("authorization", "Bearer test-secret"))
            .respond_with(RetryThenSuccess(attempts.clone()))
            .expect(2)
            .mount(&server)
            .await;
        let client = GitHubClient::new(
            config(&server),
            Some(GitHubToken::new("test-secret").unwrap()),
        )
        .unwrap();
        let url = Url::parse(&format!("{}/thread", server.uri())).unwrap();
        let response: super::GitHubResponse<Message> = client
            .get_json_page(&url, &CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(response.value.message, "ok");
        assert_eq!(response.next_page.unwrap().query(), Some("page=2"));
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn generic_forbidden_response_is_not_retried() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/private"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("Retry-After", "0")
                    .set_body_json(serde_json::json!({
                        "message": "Resource not accessible by integration"
                    })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = GitHubClient::new(config(&server), None).unwrap();
        let url = Url::parse(&format!("{}/private", server.uri())).unwrap();

        assert_eq!(
            client
                .get_json::<Message>(&url, &CancellationToken::new())
                .await,
            Err(GitHubError::Api {
                status: 403,
                kind: ApiFailureKind::PermissionDenied
            })
        );
    }

    #[tokio::test]
    async fn provider_wait_beyond_budget_is_reported_as_deferred() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/limited"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "20")
                    .set_body_json(serde_json::json!({"message": "slow down"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let mut config = config(&server);
        config.retry.total_budget = std::time::Duration::from_millis(50);
        let client = GitHubClient::new(config, None).unwrap();
        let url = Url::parse(&format!("{}/limited", server.uri())).unwrap();

        assert_eq!(
            client
                .get_json::<Message>(&url, &CancellationToken::new())
                .await,
            Err(GitHubError::Deferred {
                retry_after: Some(std::time::Duration::from_secs(20))
            })
        );
    }

    #[tokio::test]
    async fn generic_rate_limit_403_retries_but_permission_403_does_not() {
        let server = MockServer::start().await;
        let attempts = Arc::new(AtomicUsize::new(0));
        Mock::given(method("GET"))
            .and(path("/rate"))
            .respond_with(RateLimitThenSuccess(attempts.clone()))
            .expect(2)
            .mount(&server)
            .await;
        let client = GitHubClient::new(config(&server), None).unwrap();
        let url = Url::parse(&format!("{}/rate", server.uri())).unwrap();

        assert_eq!(
            client
                .get_json::<Message>(&url, &CancellationToken::new())
                .await
                .unwrap(),
            Message {
                message: "ok".to_owned()
            }
        );
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    struct RateLimitThenSuccess(Arc<AtomicUsize>);

    impl Respond for RateLimitThenSuccess {
        fn respond(&self, _request: &Request) -> ResponseTemplate {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(403)
                    .insert_header("X-RateLimit-Remaining", "0")
                    .insert_header("Retry-After", "0")
                    .set_body_json(serde_json::json!({"message": "API rate limit exceeded"}))
            } else {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"message": "ok"}))
            }
        }
    }

    #[tokio::test]
    async fn cross_origin_redirect_is_rejected_without_contacting_destination() {
        let api = MockServer::start().await;
        let destination = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/redirect"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("Location", format!("{}/leak", destination.uri())),
            )
            .expect(1)
            .mount(&api)
            .await;
        Mock::given(method("GET"))
            .and(path("/leak"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": "should not be reached"
            })))
            .expect(0)
            .mount(&destination)
            .await;
        let client =
            GitHubClient::new(config(&api), Some(GitHubToken::new("test-secret").unwrap()))
                .unwrap();
        let url = Url::parse(&format!("{}/redirect", api.uri())).unwrap();

        assert_eq!(
            client
                .get_json::<Message>(&url, &CancellationToken::new())
                .await,
            Err(GitHubError::UntrustedOrigin)
        );
    }

    #[tokio::test]
    async fn same_origin_redirect_can_follow_a_renamed_repository() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/old-name"))
            .respond_with(
                ResponseTemplate::new(301)
                    .insert_header("Location", format!("{}/repos/new-name", server.uri())),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/new-name"))
            .and(header("authorization", "Bearer test-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": "renamed"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = GitHubClient::new(
            config(&server),
            Some(GitHubToken::new("test-secret").unwrap()),
        )
        .unwrap();
        let url = Url::parse(&format!("{}/repos/old-name", server.uri())).unwrap();

        assert_eq!(
            client
                .get_json::<Message>(&url, &CancellationToken::new())
                .await
                .unwrap()
                .message,
            "renamed"
        );
    }

    #[tokio::test]
    async fn endpoint_paths_keep_enterprise_base_paths_and_encode_segments() {
        let server = MockServer::start().await;
        let base_url = Url::parse(&format!("{}/api/v3/", server.uri())).unwrap();
        let client = GitHubClient::new(ClientConfig::new(base_url), None).unwrap();

        let endpoint = client
            .endpoint_url(&["repos", "owner name", "repo"])
            .unwrap();
        assert_eq!(endpoint.path(), "/api/v3/repos/owner%20name/repo");
        assert_eq!(
            client.graphql_endpoint_url().unwrap().path(),
            "/api/graphql"
        );
    }

    #[tokio::test]
    async fn public_graphql_endpoint_uses_the_configured_origin() {
        let server = MockServer::start().await;
        let base_url = Url::parse(&format!("{}/", server.uri())).unwrap();
        let client = GitHubClient::new(ClientConfig::new(base_url), None).unwrap();

        let graphql = client.graphql_endpoint_url().unwrap();
        assert_eq!(graphql.origin().ascii_serialization(), server.uri());
        assert_eq!(graphql.path(), "/graphql");
    }

    #[tokio::test]
    async fn pagination_destination_must_match_the_api_origin() {
        let server = MockServer::start().await;
        let client = GitHubClient::new(config(&server), None).unwrap();
        let trusted = Url::parse(&format!(
            "{}/repos/example/repo/issues?page=2",
            server.uri()
        ))
        .unwrap();
        let untrusted = Url::parse("https://example.invalid/page/2").unwrap();

        assert!(client.validate_destination(&trusted).is_ok());
        assert_eq!(
            client.validate_destination(&untrusted),
            Err(GitHubError::UntrustedOrigin)
        );
        assert_eq!(
            client
                .resolve_pagination_url(&trusted, "?page=3")
                .unwrap()
                .query(),
            Some("page=3")
        );
        assert_eq!(
            client.resolve_pagination_url(&trusted, "https://example.invalid/page/3"),
            Err(GitHubError::UntrustedOrigin)
        );
    }

    #[tokio::test]
    async fn cancellation_interrupts_an_in_flight_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(std::time::Duration::from_secs(5))
                    .set_body_json(serde_json::json!({"message": "late"})),
            )
            .mount(&server)
            .await;
        let client = GitHubClient::new(config(&server), None).unwrap();
        let url = Url::parse(&format!("{}/slow", server.uri())).unwrap();
        let cancellation = CancellationToken::new();
        let request_cancellation = cancellation.clone();
        let request = tokio::spawn(async move {
            client
                .get_json::<Message>(&url, &request_cancellation)
                .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        cancellation.cancel();

        assert_eq!(request.await.unwrap(), Err(GitHubError::Cancelled));
    }

    #[test]
    fn default_policy_matches_selected_bounded_limits() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts.get(), 5);
        assert_eq!(policy.total_budget, std::time::Duration::from_secs(120));
        assert_eq!(
            GitHubClientConfig::new(Url::parse("https://api.github.com/").unwrap()).request_timeout,
            std::time::Duration::from_secs(30)
        );
    }
}
