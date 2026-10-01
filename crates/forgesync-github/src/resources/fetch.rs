//! Fetch and normalize GitHub REST resources.
//!
//! A `next_page` continuation is used unchanged; the caller keeps it paired with the scan that
//! produced it. One invalid item rejects the whole page rather than returning a subset.

use forgesync_core::content::{Comment, Discussion, PullRequestMetadata, Repository, Review};
use forgesync_core::identity::{GitHubHost, ThreadId};
use forgesync_core::timestamp::UtcTimestamp;
use reqwest::Url;
use serde::de::DeserializeOwned;
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::resources::normalize::{
    normalize_comment, normalize_issue, normalize_pull_request, normalize_repository,
    normalize_review,
};
use crate::resources::wire::{RestComment, RestIssue, RestPullRequest, RestRepository, RestReview};
use crate::resources::{Page, ThreadListState, require_thread_scope};
use crate::transport::GitHubClient;

/// Acquires and normalizes current repository metadata.
///
/// `host` scopes the normalized identity; the caller pairs it with the matching client.
pub async fn fetch_repository(
    client: &GitHubClient,
    host: &GitHubHost,
    owner: &str,
    name: &str,
    cancellation: &CancellationToken,
) -> Result<Repository, GitHubError> {
    let url = client.endpoint_url(&["repos", owner, name])?;
    let response: RestRepository = client.get_json_page(&url, cancellation).await?.value;
    normalize_repository(host, response)
}

/// Builds the first issues-endpoint page URL for a state scope, newest updates first.
///
/// `since` becomes the provider's RFC 3339 lower bound.
pub fn thread_list_url_in_scope(
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

/// Acquires one issues-and-pull-requests page; scope options only shape the first page URL.
pub async fn fetch_thread_page_in_scope(
    client: &GitHubClient,
    repository: &Repository,
    next_page: Option<&Url>,
    state: ThreadListState,
    since: Option<UtcTimestamp>,
    cancellation: &CancellationToken,
) -> Result<Page<Discussion>, GitHubError> {
    let url = match next_page {
        Some(url) => url.clone(),
        None => thread_list_url_in_scope(client, repository, state, since)?,
    };
    fetch_page(client, &url, cancellation, |issue: RestIssue| {
        normalize_issue(repository, issue)
    })
    .await
}

/// Fetches one page of [issue comments][github-comments] for a discussion.
///
/// [github-comments]: https://docs.github.com/en/rest/issues/comments#list-issue-comments
pub async fn fetch_issue_comment_page(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
    next_page: Option<&Url>,
    cancellation: &CancellationToken,
) -> Result<Page<Comment>, GitHubError> {
    require_thread_scope(repository, thread)?;
    let url = match next_page {
        Some(url) => url.clone(),
        None => first_child_page_url(client, repository, thread, "issues", "comments")?,
    };
    fetch_page(client, &url, cancellation, |comment: RestComment| {
        normalize_comment(thread, comment)
    })
    .await
}

/// Fetches base/head metadata and merge state for [one pull request][github-pull-request].
///
/// [github-pull-request]: https://docs.github.com/en/rest/pulls/pulls#get-a-pull-request
pub async fn fetch_pull_request_metadata(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
    cancellation: &CancellationToken,
) -> Result<PullRequestMetadata, GitHubError> {
    require_thread_scope(repository, thread)?;
    let number = thread.number().get().to_string();
    let url = client.endpoint_url(&[
        "repos",
        &repository.owner,
        &repository.name,
        "pulls",
        &number,
    ])?;
    let response: RestPullRequest = client.get_json_page(&url, cancellation).await?.value;
    normalize_pull_request(repository, response)
}

/// Fetches one page of [pull-request reviews][github-reviews], including pending reviews.
///
/// [github-reviews]: https://docs.github.com/en/rest/pulls/reviews#list-reviews-for-a-pull-request
pub async fn fetch_pull_request_review_page(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
    next_page: Option<&Url>,
    cancellation: &CancellationToken,
) -> Result<Page<Review>, GitHubError> {
    require_thread_scope(repository, thread)?;
    let url = match next_page {
        Some(url) => url.clone(),
        None => first_child_page_url(client, repository, thread, "pulls", "reviews")?,
    };
    fetch_page(client, &url, cancellation, |review: RestReview| {
        normalize_review(thread, review)
    })
    .await
}

/// Builds `repos/{owner}/{name}/{parent}/{number}/{child}?per_page=100`.
fn first_child_page_url(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
    parent: &str,
    child: &str,
) -> Result<Url, GitHubError> {
    let number = thread.number().get().to_string();
    let mut url = client.endpoint_url(&[
        "repos",
        &repository.owner,
        &repository.name,
        parent,
        &number,
        child,
    ])?;
    url.query_pairs_mut().append_pair("per_page", "100");
    Ok(url)
}

async fn fetch_page<W, T>(
    client: &GitHubClient,
    url: &Url,
    cancellation: &CancellationToken,
    normalize: impl FnMut(W) -> Result<T, GitHubError>,
) -> Result<Page<T>, GitHubError>
where
    W: DeserializeOwned,
{
    let response = client.get_json_page::<Vec<W>>(url, cancellation).await?;
    let items = response
        .value
        .into_iter()
        .map(normalize)
        .collect::<Result<_, _>>()?;
    Ok(Page {
        items,
        next_page: response.next_page,
    })
}
