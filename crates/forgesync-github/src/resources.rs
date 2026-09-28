use std::collections::BTreeMap;

use forgesync_core::{
    Comment, CommentId, Discussion, GitHubHost, ProviderData, ProviderId, Repository, RepositoryId,
    SourceState, ThreadId, ThreadKind, ThreadNumber, UtcTimestamp,
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{GitHubClient, GitHubError, GitHubResponse};

/// One normalized issue-list page and its validated next-page destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestThreadPage {
    /// Issues and pull requests returned by the repository issues endpoint.
    pub discussions: Vec<Discussion>,
    /// Next page URL from the provider, when one remains.
    pub next_page: Option<Url>,
}

/// One normalized issue-comment page and its validated next-page destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestCommentPage {
    /// Discussion comments returned by the issues comments endpoint.
    pub comments: Vec<Comment>,
    /// Next page URL from the provider, when one remains.
    pub next_page: Option<Url>,
}

/// Source states selected from the GitHub issues endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadListState {
    /// Include issues and pull requests in either source state.
    All,
    /// Include only currently open issues and pull requests.
    Open,
    /// Include only currently closed issues and pull requests.
    Closed,
}

/// Fetches current repository metadata from GitHub's REST API.
pub async fn fetch_repository(
    client: &GitHubClient,
    host: &GitHubHost,
    owner: &str,
    name: &str,
    cancellation: &CancellationToken,
) -> Result<Repository, GitHubError> {
    let url = client.endpoint_url(&["repos", owner, name])?;
    let response: RestRepository = client.get_json(&url, cancellation).await?;
    normalize_repository(host, response)
}

/// Fetches one page of issues and pull requests for a repository.
pub async fn fetch_thread_page(
    client: &GitHubClient,
    repository: &Repository,
    next_page: Option<&Url>,
    cancellation: &CancellationToken,
) -> Result<RestThreadPage, GitHubError> {
    fetch_thread_page_in_scope(
        client,
        repository,
        next_page,
        ThreadListState::All,
        None,
        cancellation,
    )
    .await
}

/// Fetches one page using an explicit source state and optional update-time lower bound.
pub async fn fetch_thread_page_in_scope(
    client: &GitHubClient,
    repository: &Repository,
    next_page: Option<&Url>,
    state: ThreadListState,
    since: Option<UtcTimestamp>,
    cancellation: &CancellationToken,
) -> Result<RestThreadPage, GitHubError> {
    let url = match next_page {
        Some(url) => {
            client.validate_destination(url)?;
            url.clone()
        }
        None => initial_thread_list_url(client, repository, state, since)?,
    };
    let response: GitHubResponse<Vec<RestIssue>> = client.get_json_page(&url, cancellation).await?;
    let discussions = response
        .value
        .into_iter()
        .map(|issue| normalize_issue(repository, issue))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RestThreadPage {
        discussions,
        next_page: response.next_page,
    })
}

/// Fetches one page of issue or pull-request discussion comments.
///
/// This follows GitHub's [list issue comments endpoint][github-comments], including its
/// 100-item page limit and provider-supplied pagination links.
///
/// [github-comments]: https://docs.github.com/en/rest/issues/comments#list-issue-comments
pub async fn fetch_issue_comment_page(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
    next_page: Option<&Url>,
    cancellation: &CancellationToken,
) -> Result<RestCommentPage, GitHubError> {
    if thread.repository() != &repository.id {
        return Err(GitHubError::InvalidProviderData);
    }
    let url = match next_page {
        Some(url) => {
            client.validate_destination(url)?;
            url.clone()
        }
        None => initial_issue_comment_url(client, repository, thread)?,
    };
    let response: GitHubResponse<Vec<RestComment>> =
        client.get_json_page(&url, cancellation).await?;
    let comments = response
        .value
        .into_iter()
        .map(|comment| normalize_comment(thread, comment))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RestCommentPage {
        comments,
        next_page: response.next_page,
    })
}

/// Builds the first issue-comment page URL for a discussion.
pub fn issue_comment_list_url(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
) -> Result<Url, GitHubError> {
    initial_issue_comment_url(client, repository, thread)
}

fn initial_issue_comment_url(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
) -> Result<Url, GitHubError> {
    let number = thread.number().get().to_string();
    let mut url = client.endpoint_url(&[
        "repos",
        &repository.owner,
        &repository.name,
        "issues",
        &number,
        "comments",
    ])?;
    url.query_pairs_mut().append_pair("per_page", "100");
    Ok(url)
}

fn initial_thread_list_url(
    client: &GitHubClient,
    repository: &Repository,
    state: ThreadListState,
    since: Option<UtcTimestamp>,
) -> Result<Url, GitHubError> {
    let mut url = client.endpoint_url(&["repos", &repository.owner, &repository.name, "issues"])?;
    let state = match state {
        ThreadListState::All => "all",
        ThreadListState::Open => "open",
        ThreadListState::Closed => "closed",
    };
    url.query_pairs_mut()
        .append_pair("state", state)
        .append_pair("sort", "updated")
        .append_pair("direction", "desc")
        .append_pair("per_page", "100");
    if let Some(since) = since {
        let since = since
            .format_rfc3339()
            .map_err(|_| GitHubError::InvalidProviderData)?;
        url.query_pairs_mut().append_pair("since", &since);
    }
    Ok(url)
}

/// Builds the first issues-and-pull-requests page URL for a repository.
pub fn thread_list_url(client: &GitHubClient, repository: &Repository) -> Result<Url, GitHubError> {
    initial_thread_list_url(client, repository, ThreadListState::All, None)
}

/// Builds the first issues page URL for a selected state and optional update-time lower bound.
pub fn thread_list_url_in_scope(
    client: &GitHubClient,
    repository: &Repository,
    state: ThreadListState,
    since: Option<UtcTimestamp>,
) -> Result<Url, GitHubError> {
    initial_thread_list_url(client, repository, state, since)
}

fn normalize_repository(
    host: &GitHubHost,
    repository: RestRepository,
) -> Result<Repository, GitHubError> {
    let owner = repository
        .owner
        .login
        .as_deref()
        .filter(|login| !login.is_empty())
        .ok_or(GitHubError::InvalidProviderData)?;
    let full_name = repository
        .full_name
        .unwrap_or_else(|| format!("{owner}/{}", repository.name));
    let mut provider_data = provider_data(repository.extra);
    provider_data.insert(
        "owner",
        serde_json::to_value(&repository.owner).map_err(json_error)?,
    );
    let id = RepositoryId::new(
        host.clone(),
        ProviderId::new(repository.id.to_string()).map_err(|_| GitHubError::InvalidProviderData)?,
    );
    Ok(Repository {
        id,
        owner: owner.to_owned(),
        name: repository.name,
        full_name,
        default_branch: repository.default_branch,
        updated_at: repository.updated_at.map(parse_timestamp).transpose()?,
        provider_data,
    })
}

fn normalize_issue(repository: &Repository, issue: RestIssue) -> Result<Discussion, GitHubError> {
    let id = ThreadId::new(
        repository.id.clone(),
        ProviderId::new(issue.id.to_string()).map_err(|_| GitHubError::InvalidProviderData)?,
        ThreadNumber::new(issue.number).map_err(|_| GitHubError::InvalidProviderData)?,
    );
    let source_state = match issue.state.as_str() {
        "open" => SourceState::Open,
        "closed" => SourceState::Closed,
        state => SourceState::Other(state.to_owned()),
    };
    let labels = issue
        .labels
        .iter()
        .flatten()
        .map(|label| label.name.clone())
        .collect::<Vec<_>>();
    let assignees = issue
        .assignees
        .iter()
        .flatten()
        .filter_map(|assignee| assignee.login.clone())
        .collect::<Vec<_>>();
    let mut provider_data = provider_data(issue.extra);
    if let Some(user) = issue.user {
        provider_data.insert("user", serde_json::to_value(user).map_err(json_error)?);
    }
    if let Some(labels) = issue.labels {
        provider_data.insert(
            "labels_source",
            serde_json::to_value(labels).map_err(json_error)?,
        );
    }
    if let Some(assignees) = issue.assignees {
        provider_data.insert(
            "assignees_source",
            serde_json::to_value(assignees).map_err(json_error)?,
        );
    }
    if let Some(pull_request) = issue.pull_request {
        provider_data.insert("pull_request", pull_request);
    }

    Ok(Discussion {
        id,
        kind: if provider_data.get("pull_request").is_some() {
            ThreadKind::PullRequest
        } else {
            ThreadKind::Issue
        },
        state: source_state,
        title: issue.title,
        body: issue.body,
        html_url: issue.html_url,
        created_at: parse_timestamp(issue.created_at)?,
        updated_at: parse_timestamp(issue.updated_at)?,
        closed_at: issue.closed_at.map(parse_timestamp).transpose()?,
        labels,
        assignees,
        provider_data,
    })
}

fn normalize_comment(thread: &ThreadId, comment: RestComment) -> Result<Comment, GitHubError> {
    let provider_id =
        ProviderId::new(comment.id.to_string()).map_err(|_| GitHubError::InvalidProviderData)?;
    let created_at =
        UtcTimestamp::parse(&comment.created_at).map_err(|_| GitHubError::InvalidProviderData)?;
    let updated_at = comment
        .updated_at
        .map(|value| UtcTimestamp::parse(&value).map_err(|_| GitHubError::InvalidProviderData))
        .transpose()?;
    let author = comment
        .user
        .as_ref()
        .and_then(|user| user.get("login"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut provider_data = provider_data(comment.extra);
    if let Some(user) = comment.user {
        provider_data.insert("user", user);
    }
    Ok(Comment {
        id: CommentId::new(thread.clone(), provider_id),
        review_id: None,
        author,
        body: comment.body,
        created_at,
        updated_at,
        provider_data,
    })
}

fn parse_timestamp(value: String) -> Result<UtcTimestamp, GitHubError> {
    UtcTimestamp::parse(&value).map_err(|_| GitHubError::InvalidProviderData)
}

fn provider_data(extra: BTreeMap<String, Value>) -> ProviderData {
    let mut provider_data = ProviderData::new();
    for (name, value) in extra {
        provider_data.insert(name, value);
    }
    provider_data
}

fn json_error(_: serde_json::Error) -> GitHubError {
    GitHubError::InvalidProviderData
}

#[derive(Deserialize)]
struct RestRepository {
    id: u64,
    name: String,
    full_name: Option<String>,
    owner: RestUser,
    default_branch: Option<String>,
    updated_at: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
struct RestIssue {
    id: u64,
    number: u64,
    state: String,
    title: String,
    body: Option<String>,
    created_at: String,
    updated_at: String,
    closed_at: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    labels: Option<Vec<RestLabel>>,
    #[serde(default)]
    assignees: Option<Vec<RestUser>>,
    user: Option<RestUser>,
    pull_request: Option<Value>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct RestComment {
    id: u64,
    #[serde(default)]
    body: String,
    created_at: String,
    updated_at: Option<String>,
    user: Option<Value>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
struct RestUser {
    login: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
struct RestLabel {
    name: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use forgesync_core::{
        GitHubHost, ProviderData, ProviderId, Repository, RepositoryId, ThreadId, ThreadKind,
        ThreadNumber, UtcTimestamp,
    };
    use reqwest::Url;
    use serde_json::json;
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::{GitHubClient, GitHubClientConfig};

    use super::{fetch_issue_comment_page, fetch_repository, fetch_thread_page};

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

        let page = fetch_thread_page(&client, &repository, None, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(page.discussions.len(), 2);
        assert_eq!(page.discussions[0].id.number().get(), 17);
        assert_eq!(page.discussions[0].kind, ThreadKind::Issue);
        assert_eq!(page.discussions[1].kind, ThreadKind::PullRequest);
        assert_eq!(page.discussions[1].labels, ["feature"]);
        assert_eq!(page.discussions[1].assignees, ["sample-reviewer"]);
        assert_eq!(
            page.discussions[1].provider_data.get("node_id"),
            Some(&json!("PR_fixture_1802"))
        );
        assert_eq!(
            page.discussions[1]
                .provider_data
                .get("labels_source")
                .unwrap()[0]["color"],
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
        let page = fetch_thread_page(&client, &repository, None, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(page.discussions.len(), 1);
        assert_eq!(page.next_page.unwrap().query(), Some("page=2"));
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
        assert_eq!(first.comments.len(), 1);
        assert_eq!(first.comments[0].id.thread(), &thread);
        assert_eq!(first.comments[0].author.as_deref(), Some("reviewer"));
        assert_eq!(
            first.comments[0].provider_data.get("author_association"),
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
        assert_eq!(second.comments.len(), 1);
        assert_eq!(second.comments[0].id.provider_id().as_str(), "3002");
        assert_eq!(second.comments[0].author, None);
        assert_eq!(second.comments[0].updated_at, None);
        assert_eq!(
            second.comments[0].provider_data.get("node_id"),
            Some(&json!("IC_fixture_3002"))
        );
        assert!(second.next_page.is_none());
    }
}
