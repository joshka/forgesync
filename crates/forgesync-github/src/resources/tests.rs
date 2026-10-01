//! REST acquisition and normalization against fixture payloads, including enterprise base paths
//! and pagination links.

use std::path::Path;

use forgesync_core::content::{Repository, ReviewState, ThreadKind};
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use reqwest::Url;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::resources::{
    ThreadListState, fetch_issue_comment_page, fetch_pull_request_metadata,
    fetch_pull_request_review_page, fetch_repository, fetch_thread_page_in_scope,
};
use crate::transport::{GitHubClient, GitHubClientConfig};

/// Loads a synthetic provider payload by its catalog filename.
fn fixture(name: &str) -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/github")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture contents"))
        .expect("fixture JSON")
}

#[tokio::test]
async fn repository_and_thread_rest_responses_normalize_stable_provider_identity() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/fixture-lab/archive-demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json({
            let mut repository = fixture("repository.json");
            repository["owner"] = json!({ "login": "fixture-lab" });
            repository
        }))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/fixture-lab/archive-demo/issues"))
        .and(query_param("state", "all"))
        .and(query_param("per_page", "100"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([fixture("issue.json"), {
                    "id": 1802,
                    "number": 18,
                    "state": "closed",
                    "title": "PR thread",
                    "body": null,
                    "created_at": "2026-09-18T10:00:00Z",
                    "updated_at": "2026-09-20T10:00:00Z",
                    "closed_at": "2026-09-20T11:00:00Z",
                    "html_url": "https://github.com/fixture-lab/archive-demo/pull/18",
                    "pull_request": {"url": "https://api.github.com/repos/fixture-lab/archive-demo/pulls/18"},
                    "node_id": "PR_fixture_1802",
                    "labels": [{"name": "feature", "color": "abcdef"}],
                    "assignees": [{"login": "sample-reviewer", "type": "User"}]
                }])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = GitHubClient::new(
        GitHubClientConfig::new(Url::parse(&format!("{}/api/v3/", server.uri())).unwrap()),
        None,
    )
    .unwrap();
    let host = GitHubHost::parse("github.com").unwrap();
    let repository = fetch_repository(
        &client,
        &host,
        "fixture-lab",
        "archive-demo",
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(repository.id.provider_id().as_str(), "41");
    assert_eq!(repository.full_name, "fixture-lab/archive-demo");
    assert_eq!(
        repository.provider_data.get("node_id"),
        Some(&json!("R_fixture_41"))
    );

    let page = fetch_thread_page_in_scope(
        &client,
        &repository,
        None,
        ThreadListState::All,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id.number().get(), 17);
    assert_eq!(page.items[0].kind, ThreadKind::Issue);
    assert_eq!(page.items[1].kind, ThreadKind::PullRequest);
    assert_eq!(page.items[1].labels, ["feature"]);
    assert_eq!(page.items[1].assignees, ["sample-reviewer"]);
    assert_eq!(
        page.items[1].provider_data.get("node_id"),
        Some(&json!("PR_fixture_1802"))
    );
    assert_eq!(
        page.items[1].provider_data.get("labels_source").unwrap()[0]["color"],
        "abcdef"
    );
}

#[tokio::test]
async fn thread_page_retains_enterprise_base_path_and_next_link() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/fixture-lab/archive-demo/issues"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", "<?page=2>; rel=\"next\"")
                .set_body_json(json!([fixture("issue.json")])),
        )
        .mount(&server)
        .await;
    let client = GitHubClient::new(
        GitHubClientConfig::new(Url::parse(&format!("{}/api/v3/", server.uri())).unwrap()),
        None,
    )
    .unwrap();
    let host = GitHubHost::parse("ghe.example.test").unwrap();
    let repository = Repository {
        id: RepositoryId::new(host, ProviderId::new("41").unwrap()),
        owner: "fixture-lab".to_owned(),
        name: "archive-demo".to_owned(),
        full_name: "fixture-lab/archive-demo".to_owned(),
        default_branch: Some("main".to_owned()),
        updated_at: Some(UtcTimestamp::parse("2026-09-20T12:00:00Z").unwrap()),
        provider_data: ProviderData::new(),
    };
    let page = fetch_thread_page_in_scope(
        &client,
        &repository,
        None,
        ThreadListState::All,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);
    let expected_next = Url::parse(&format!(
        "{}/api/v3/repos/fixture-lab/archive-demo/issues?page=2",
        server.uri()
    ))
    .expect("next-page URL");
    assert_eq!(page.next_page, Some(expected_next));
}

#[tokio::test]
async fn issue_comment_pages_normalize_identity_and_preserve_unknown_fields() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/enterprise/api/v3/repos/fixture-lab/archive-demo/issues/17/comments",
        ))
        .and(query_param("per_page", "100"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", "<?page=2>; rel=\"next\"")
                .set_body_json(json!([{
                    "id": 3001,
                    "body": "first response",
                    "created_at": "2026-09-19T08:00:00Z",
                    "updated_at": "2026-09-19T09:00:00Z",
                    "user": { "login": "reviewer", "type": "User" },
                    "author_association": "CONTRIBUTOR",
                    "reactions": { "+1": 2 }
                }])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/enterprise/api/v3/repos/fixture-lab/archive-demo/issues/17/comments",
        ))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 3002,
            "body": "second response",
            "created_at": "2026-09-19T10:00:00Z",
            "updated_at": null,
            "user": null,
            "node_id": "IC_fixture_3002"
        }])))
        .expect(1)
        .mount(&server)
        .await;
    let client = GitHubClient::new(
        GitHubClientConfig::new(
            Url::parse(&format!("{}/enterprise/api/v3/", server.uri())).unwrap(),
        ),
        None,
    )
    .expect("GitHub client");
    let repository_id = RepositoryId::new(
        GitHubHost::parse("ghe.example.test").unwrap(),
        ProviderId::new("41").unwrap(),
    );
    let repository = Repository {
        id: repository_id.clone(),
        owner: "fixture-lab".to_owned(),
        name: "archive-demo".to_owned(),
        full_name: "fixture-lab/archive-demo".to_owned(),
        default_branch: Some("main".to_owned()),
        updated_at: None,
        provider_data: ProviderData::new(),
    };
    let thread = ThreadId::new(
        repository_id,
        ProviderId::new("1701").unwrap(),
        ThreadNumber::new(17).unwrap(),
    );
    let first = fetch_issue_comment_page(
        &client,
        &repository,
        &thread,
        None,
        &CancellationToken::new(),
    )
    .await
    .expect("first comment page");
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.items[0].id.thread(), &thread);
    assert_eq!(first.items[0].id.provider_id().as_str(), "3001");
    assert_eq!(first.items[0].body, "first response");
    assert_eq!(first.items[0].author.as_deref(), Some("reviewer"));
    assert_eq!(
        first.items[0].provider_data.get("author_association"),
        Some(&json!("CONTRIBUTOR"))
    );
    assert!(first.next_page.is_some());

    let second = fetch_issue_comment_page(
        &client,
        &repository,
        &thread,
        first.next_page.as_ref(),
        &CancellationToken::new(),
    )
    .await
    .expect("second comment page");
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].id.provider_id().as_str(), "3002");
    assert_eq!(second.items[0].id.thread(), &thread);
    assert_eq!(second.items[0].body, "second response");
    assert_eq!(second.items[0].author, None);
    assert_eq!(second.items[0].updated_at, None);
    assert_eq!(
        second.items[0].provider_data.get("node_id"),
        Some(&json!("IC_fixture_3002"))
    );
    assert!(second.next_page.is_none());
}

#[tokio::test]
async fn pull_request_metadata_and_reviews_keep_head_and_reviewer_provenance() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/enterprise/api/v3/repos/fixture-lab/archive-demo/pulls/18",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "base": {
                "ref": "main",
                "sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "repo": { "id": 41, "full_name": "fixture-lab/archive-demo" }
            },
            "head": {
                "ref": "topic",
                "sha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "repo": { "id": 99, "full_name": "contributor/archive-demo" }
            },
            "draft": true,
            "merged": false,
            "maintainer_can_modify": true
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/enterprise/api/v3/repos/fixture-lab/archive-demo/pulls/18/reviews",
        ))
        .and(query_param("per_page", "100"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", "<?page=2>; rel=\"next\"")
                .set_body_json(json!([{
                    "id": 1801,
                    "state": "APPROVED",
                    "body": "Looks good.",
                    "submitted_at": "2026-09-19T12:00:00Z",
                    "commit_id": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "user": { "id": 51, "login": "reviewer", "type": "User" },
                    "author_association": "MEMBER"
                }])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/enterprise/api/v3/repos/fixture-lab/archive-demo/pulls/18/reviews",
        ))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": 1802,
            "state": "PENDING",
            "body": null,
            "submitted_at": null,
            "commit_id": null,
            "user": null,
            "node_id": "PRR_fixture_1802"
        }])))
        .expect(1)
        .mount(&server)
        .await;
    let client = GitHubClient::new(
        GitHubClientConfig::new(
            Url::parse(&format!("{}/enterprise/api/v3/", server.uri())).unwrap(),
        ),
        None,
    )
    .expect("GitHub client");
    let repository_id = RepositoryId::new(
        GitHubHost::parse("ghe.example.test").unwrap(),
        ProviderId::new("41").unwrap(),
    );
    let repository = Repository {
        id: repository_id.clone(),
        owner: "fixture-lab".to_owned(),
        name: "archive-demo".to_owned(),
        full_name: "fixture-lab/archive-demo".to_owned(),
        default_branch: Some("main".to_owned()),
        updated_at: None,
        provider_data: ProviderData::new(),
    };
    let thread = ThreadId::new(
        repository_id,
        ProviderId::new("1802").unwrap(),
        ThreadNumber::new(18).unwrap(),
    );
    let cancellation = CancellationToken::new();

    let metadata = fetch_pull_request_metadata(&client, &repository, &thread, &cancellation)
        .await
        .expect("pull request metadata");
    assert_eq!(metadata.base.name, "main");
    assert_eq!(
        metadata
            .base
            .repository
            .as_ref()
            .unwrap()
            .provider_id()
            .as_str(),
        "41"
    );
    assert_eq!(
        metadata.head.sha.as_str(),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    assert_eq!(
        metadata
            .head
            .repository
            .as_ref()
            .unwrap()
            .provider_id()
            .as_str(),
        "99"
    );
    assert!(metadata.draft);
    assert!(!metadata.merged);
    assert_eq!(
        metadata.provider_data.get("maintainer_can_modify"),
        Some(&json!(true))
    );
    assert_eq!(
        metadata.provider_data.get("head_source").unwrap()["ref"],
        "topic"
    );

    let first = fetch_pull_request_review_page(&client, &repository, &thread, None, &cancellation)
        .await
        .expect("first review page");
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.items[0].id.thread(), &thread);
    assert_eq!(first.items[0].state, ReviewState::Approved);
    let reviewer = first.items[0].reviewer.as_ref().expect("reviewer identity");
    assert_eq!(reviewer.provider_id.as_ref().unwrap().as_str(), "51");
    assert_eq!(reviewer.login.as_deref(), Some("reviewer"));
    assert_eq!(reviewer.provider_data.get("type"), Some(&json!("User")));
    assert_eq!(
        first.items[0].provider_data.get("author_association"),
        Some(&json!("MEMBER"))
    );
    assert!(first.next_page.is_some());

    let second = fetch_pull_request_review_page(
        &client,
        &repository,
        &thread,
        first.next_page.as_ref(),
        &cancellation,
    )
    .await
    .expect("second review page");
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].state, ReviewState::Pending);
    assert!(second.items[0].reviewer.is_none());
    assert!(second.items[0].commit_sha.is_none());
    assert!(second.next_page.is_none());
}
