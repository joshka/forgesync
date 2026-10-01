//! Construct and fetch the selected GitHub REST resource page.
//!
//! Each `fetch_*` operation takes a configured [`crate::transport::GitHubClient`] and explicit
//! cancellation. URL builders form first-page endpoints from checked repository and thread scope;
//! subsequent pages come from transport-validated pagination links. Scope checks reject a thread
//! whose repository identity differs from the selected repository.
//!
//! The returned page contains normalized resources, not an archive transaction. The engine
//! controls when to persist a page and cursor, and whether a family has reached its final page.
//! This matters for child collections: an interrupted page sequence must not replace older
//! complete membership.
//!
//! Use `fetch_repository` before thread enumeration when current repository name or identity is
//! needed. Use the family-specific fetch functions for later comment, metadata, and review jobs
//! rather than treating all resources as interchangeable provider JSON.

use forgesync_core::content::{PullRequestMetadata, Repository};
use forgesync_core::identity::{GitHubHost, ThreadId};
use forgesync_core::timestamp::UtcTimestamp;
use reqwest::Url;
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::resources::normalize::{
    normalize_comment, normalize_issue, normalize_pull_request, normalize_repository,
    normalize_review,
};
use crate::resources::urls::{
    initial_issue_comment_url, initial_pull_request_review_url, initial_thread_list_url,
};
use crate::resources::wire::{RestComment, RestIssue, RestPullRequest, RestRepository, RestReview};
use crate::resources::{RestCommentPage, RestReviewPage, RestThreadPage, ThreadListState};
use crate::transport::{GitHubClient, GitHubResponse};

/// Acquires and normalizes current repository metadata from the configured REST API.
///
/// `owner` and `name` select encoded endpoint segments. `host` supplies the normalized identity
/// scope; the caller must pair it with the correct configured client because this function does
/// not compare that domain host with the transport API origin. No archive registration occurs.
///
/// # Errors
///
/// Returns transport/cancellation failures or invalid-provider-data errors when required identity,
/// owner, or timestamp fields cannot be normalized.
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

/// Acquires one issues-and-pull-requests page across all source states, without a time lower bound.
///
/// Pass `None` to start at the repository endpoint or a continuation from the same selected scan.
/// This convenience operation uses [`fetch_thread_page_in_scope`] and preserves its normalization,
/// cancellation, and completeness limits.
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

/// Acquires one discussion page for an explicit state and optional source-update lower bound.
///
/// Scope options construct the initial URL only. When `next_page` is present, its validated
/// destination is used unchanged; the caller must retain the repository and scan scope that
/// produced it. Origin validation alone does not prove that an arbitrary same-origin URL belongs to
/// this scan.
///
/// A successful result normalizes every item on this page and retains the provider continuation.
/// It writes no cursor or membership. An empty page or absent continuation does not establish that
/// earlier pages were committed; engine/store accounting owns complete-collection authority.
///
/// # Errors
///
/// Returns URL, transport, cancellation, pagination, or normalization failures. One invalid item
/// rejects this page result rather than returning an apparently complete subset.
pub async fn fetch_thread_page_in_scope(
    client: &GitHubClient,
    repository: &Repository,
    next_page: Option<&Url>,
    state: ThreadListState,
    since: Option<UtcTimestamp>,
    cancellation: &CancellationToken,
) -> Result<RestThreadPage, GitHubError> {
    let url = match next_page {
        Some(url) => url.clone(),
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
/// The repository identity must match the supplied thread. A continuation must come from this
/// discussion's comment scan; only its origin is validated here. All returned comments are
/// normalized before success, while durable membership and collection completeness remain
/// engine/store policy. Transport, cancellation, scope mismatch, and invalid comment fields return
/// typed errors.
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
        Some(url) => url.clone(),
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
/// Checks the supplied repository/thread identity before requesting the numbered pull endpoint.
/// This type-level check cannot prove pull-request kind or source existence. The result contains
/// normalized branch revisions and source draft/merge facts, without changing archive coverage.
/// Scope, transport, cancellation, and malformed provider metadata return typed errors.
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

/// Fetches one page of pull-request reviews using GitHub's [list reviews endpoint][github-reviews]
/// and its 100-item page limit.
///
/// The thread must have the selected repository identity. Continuation uses the supplied URL
/// unchanged after origin validation; the caller keeps it paired with this review scope. Pending
/// reviews and missing reviewer/body fields retain the normalizer's source meaning. One malformed
/// review rejects this page; no durable membership is written by fetching it.
///
/// [github-reviews]: https://docs.github.com/en/rest/pulls/reviews#list-reviews-for-a-pull-request
pub async fn fetch_pull_request_review_page(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
    next_page: Option<&Url>,
    cancellation: &CancellationToken,
) -> Result<RestReviewPage, GitHubError> {
    require_thread_scope(repository, thread)?;
    let url = match next_page {
        Some(url) => url.clone(),
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

/// Checks only that a thread identity has the selected repository identity.
///
/// This comparison proves no archive/provider existence and cannot verify pull-request kind:
/// [`ThreadId`] contains identity and number, not normalized source kind. A mismatched repository
/// returns [`GitHubError::InvalidProviderData`] before acquisition.
pub fn require_thread_scope(repository: &Repository, thread: &ThreadId) -> Result<(), GitHubError> {
    if thread.repository() != &repository.id {
        return Err(GitHubError::InvalidProviderData);
    }
    Ok(())
}
