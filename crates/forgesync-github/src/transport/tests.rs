use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use reqwest::Url;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use super::{GitHubClientConfig as ClientConfig, RetryPolicy};
use crate::error::{ApiFailureKind, GitHubError};
use crate::token::GitHubToken;
use crate::transport::{GitHubClient, GitHubClientConfig};

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
        GitHubClient::new(config(&api), Some(GitHubToken::new("test-secret").unwrap())).unwrap();
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
