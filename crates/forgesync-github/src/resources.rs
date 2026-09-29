use std::collections::BTreeMap;

use forgesync_core::{
    BranchRef, Comment, CommentId, CommitSha, Discussion, GitHubHost, ProviderData, ProviderId,
    PullRequestMetadata, Repository, RepositoryId, Review, ReviewId, ReviewState, ReviewerIdentity,
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

/// One normalized pull-request review page and its validated next-page destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestReviewPage {
    /// Submitted and pending reviews returned by the pull-request reviews endpoint.
    pub reviews: Vec<Review>,
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

/// Fetches and normalizes base/head metadata and merge state for one pull request using GitHub's
/// [get pull request endpoint][github-pull-request].
///
/// [github-pull-request]: https://docs.github.com/en/rest/pulls/pulls#get-a-pull-request
pub async fn fetch_pull_request_metadata(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
    cancellation: &CancellationToken,
) -> Result<PullRequestMetadata, GitHubError> {
    validate_pull_request_scope(repository, thread)?;
    let number = thread.number().get().to_string();
    let url = client.endpoint_url(&[
        "repos",
        &repository.owner,
        &repository.name,
        "pulls",
        &number,
    ])?;
    let response: RestPullRequest = client.get_json(&url, cancellation).await?;
    normalize_pull_request(repository, response)
}

/// Fetches one page of pull-request reviews using GitHub's [list reviews endpoint][github-reviews]
/// and its 100-item page limit.
///
/// [github-reviews]: https://docs.github.com/en/rest/pulls/reviews#list-reviews-for-a-pull-request
pub async fn fetch_pull_request_review_page(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
    next_page: Option<&Url>,
    cancellation: &CancellationToken,
) -> Result<RestReviewPage, GitHubError> {
    validate_pull_request_scope(repository, thread)?;
    let url = match next_page {
        Some(url) => {
            client.validate_destination(url)?;
            url.clone()
        }
        None => initial_pull_request_review_url(client, repository, thread)?,
    };
    let response: GitHubResponse<Vec<RestReview>> =
        client.get_json_page(&url, cancellation).await?;
    let reviews = response
        .value
        .into_iter()
        .map(|review| normalize_review(thread, review))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RestReviewPage {
        reviews,
        next_page: response.next_page,
    })
}

fn validate_pull_request_scope(
    repository: &Repository,
    thread: &ThreadId,
) -> Result<(), GitHubError> {
    if thread.repository() != &repository.id {
        return Err(GitHubError::InvalidProviderData);
    }
    Ok(())
}

fn initial_pull_request_review_url(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
) -> Result<Url, GitHubError> {
    let number = thread.number().get().to_string();
    let mut url = client.endpoint_url(&[
        "repos",
        &repository.owner,
        &repository.name,
        "pulls",
        &number,
        "reviews",
    ])?;
    url.query_pairs_mut().append_pair("per_page", "100");
    Ok(url)
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

fn normalize_pull_request(
    repository: &Repository,
    pull_request: RestPullRequest,
) -> Result<PullRequestMetadata, GitHubError> {
    let base_source = serde_json::to_value(&pull_request.base).map_err(json_error)?;
    let head_source = serde_json::to_value(&pull_request.head).map_err(json_error)?;
    let base = normalize_branch_ref(repository.id.host(), pull_request.base)?;
    let head = normalize_branch_ref(repository.id.host(), pull_request.head)?;
    let mut provider_data = provider_data(pull_request.extra);
    provider_data.insert("base_source", base_source);
    provider_data.insert("head_source", head_source);

    Ok(PullRequestMetadata {
        base,
        head,
        draft: pull_request.draft,
        merged: pull_request.merged,
        provider_data,
    })
}

fn normalize_branch_ref(
    host: &GitHubHost,
    branch: RestBranchRef,
) -> Result<BranchRef, GitHubError> {
    let repository = branch
        .repo
        .and_then(|repository| repository.id)
        .map(|id| {
            let provider_id =
                ProviderId::new(id.to_string()).map_err(|_| GitHubError::InvalidProviderData)?;
            Ok::<_, GitHubError>(RepositoryId::new(host.clone(), provider_id))
        })
        .transpose()?;
    let sha = CommitSha::new(branch.sha).map_err(|_| GitHubError::InvalidProviderData)?;
    Ok(BranchRef {
        name: branch.name,
        sha,
        repository,
    })
}

fn normalize_review(thread: &ThreadId, review: RestReview) -> Result<Review, GitHubError> {
    let provider_id =
        ProviderId::new(review.id.to_string()).map_err(|_| GitHubError::InvalidProviderData)?;
    let submitted_at = review.submitted_at.map(parse_timestamp).transpose()?;
    let commit_sha = review
        .commit_id
        .filter(|value| !value.is_empty())
        .map(CommitSha::new)
        .transpose()
        .map_err(|_| GitHubError::InvalidProviderData)?;
    let reviewer = review.user.as_ref().and_then(normalize_reviewer);
    let mut provider_data = provider_data(review.extra);
    if let Some(user) = review.user {
        provider_data.insert("user", user);
    }
    Ok(Review {
        id: ReviewId::new(thread.clone(), provider_id),
        state: normalize_review_state(&review.state),
        reviewer,
        body: review.body,
        submitted_at,
        commit_sha,
        provider_data,
    })
}

fn normalize_reviewer(user: &Value) -> Option<ReviewerIdentity> {
    let object = user.as_object()?;
    let provider_id = object.get("id").and_then(provider_id_from_value);
    let login = object
        .get("login")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let provider_data = ProviderData::from_value(user.clone()).unwrap_or_default();
    Some(ReviewerIdentity {
        provider_id,
        login,
        provider_data,
    })
}

fn provider_id_from_value(value: &Value) -> Option<ProviderId> {
    let value = value
        .as_u64()
        .map(|id| id.to_string())
        .or_else(|| value.as_str().map(str::to_owned))?;
    ProviderId::new(value).ok()
}

fn normalize_review_state(state: &str) -> ReviewState {
    match state {
        "APPROVED" => ReviewState::Approved,
        "CHANGES_REQUESTED" => ReviewState::ChangesRequested,
        "COMMENTED" => ReviewState::Commented,
        "DISMISSED" => ReviewState::Dismissed,
        "PENDING" => ReviewState::Pending,
        state => ReviewState::Other(state.to_owned()),
    }
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

#[derive(Deserialize)]
struct RestPullRequest {
    base: RestBranchRef,
    head: RestBranchRef,
    draft: bool,
    merged: bool,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
struct RestBranchRef {
    #[serde(rename = "ref")]
    name: String,
    sha: String,
    repo: Option<RestBranchRepository>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
struct RestBranchRepository {
    id: Option<u64>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct RestReview {
    id: u64,
    state: String,
    body: Option<String>,
    submitted_at: Option<String>,
    commit_id: Option<String>,
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
        GitHubHost, ProviderData, ProviderId, Repository, RepositoryId, ReviewState, ThreadId,
        ThreadKind, ThreadNumber, UtcTimestamp,
    };
    use reqwest::Url;
    use serde_json::json;
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        fetch_issue_comment_page, fetch_pull_request_metadata, fetch_pull_request_review_page,
        fetch_repository, fetch_thread_page,
    };
    use crate::{GitHubClient, GitHubClientConfig};

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

        let first =
            fetch_pull_request_review_page(&client, &repository, &thread, None, &cancellation)
                .await
                .expect("first review page");
        assert_eq!(first.reviews.len(), 1);
        assert_eq!(first.reviews[0].id.thread(), &thread);
        assert_eq!(first.reviews[0].state, ReviewState::Approved);
        let reviewer = first.reviews[0]
            .reviewer
            .as_ref()
            .expect("reviewer identity");
        assert_eq!(reviewer.provider_id.as_ref().unwrap().as_str(), "51");
        assert_eq!(reviewer.login.as_deref(), Some("reviewer"));
        assert_eq!(reviewer.provider_data.get("type"), Some(&json!("User")));
        assert_eq!(
            first.reviews[0].provider_data.get("author_association"),
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
        assert_eq!(second.reviews.len(), 1);
        assert_eq!(second.reviews[0].state, ReviewState::Pending);
        assert!(second.reviews[0].reviewer.is_none());
        assert!(second.reviews[0].commit_sha.is_none());
        assert!(second.next_page.is_none());
    }
}
