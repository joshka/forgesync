//! REST acquisition and normalization for repositories, threads, comments, and reviews.

use std::collections::BTreeMap;

use forgesync_core::content::{
    BranchRef, Comment, Discussion, PullRequestMetadata, Repository, Review, ReviewState,
    ReviewerIdentity, SourceState, ThreadKind,
};
use forgesync_core::identity::{
    CommentId, CommitSha, GitHubHost, ProviderId, RepositoryId, ReviewId, ThreadId, ThreadNumber,
};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::transport::{GitHubClient, GitHubResponse};

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
mod tests;
