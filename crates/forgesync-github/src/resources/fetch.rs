//! Resource endpoints and pagination URLs.

use super::normalize::{
    normalize_comment, normalize_issue, normalize_pull_request, normalize_repository,
    normalize_review,
};
use super::{
    CancellationToken, GitHubClient, GitHubError, GitHubHost, GitHubResponse, PullRequestMetadata,
    Repository, RestComment, RestCommentPage, RestIssue, RestPullRequest, RestRepository,
    RestReview, RestReviewPage, RestThreadPage, ThreadId, ThreadListState, Url, UtcTimestamp,
};

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

pub fn validate_pull_request_scope(
    repository: &Repository,
    thread: &ThreadId,
) -> Result<(), GitHubError> {
    if thread.repository() != &repository.id {
        return Err(GitHubError::InvalidProviderData);
    }
    Ok(())
}

pub fn initial_pull_request_review_url(
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

pub fn initial_issue_comment_url(
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

pub fn initial_thread_list_url(
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
