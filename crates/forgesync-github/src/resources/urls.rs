//! # Construct initial REST URLs without acquiring resources
//!
//! Discussion listing, issue comments, and pull-request reviews have explicit first-page URL
//! builders. They append encoded endpoint segments to the configured client base and select the
//! existing 100-item page size. Thread enumeration additionally selects source state and descending
//! update order, with an optional source-time lower bound.
//!
//! These builders perform no network request or archive lookup. They use repository display paths
//! and checked discussion numbers; they do not prove that those values identify an existing
//! resource or that a thread belongs to the supplied repository. Acquisition owns the scope check.
//!
//! Provider continuation links are handled by transport and acquisition rather than reconstructed
//! here. Rebuilding a first-page URL while continuing a scan would discard the provider cursor and
//! can change the selected scope. Public list builders expose the initial URL for workflow cursors.

use forgesync_core::content::Repository;
use forgesync_core::identity::ThreadId;
use forgesync_core::timestamp::UtcTimestamp;
use reqwest::Url;

use crate::error::GitHubError;
use crate::resources::ThreadListState;
use crate::transport::GitHubClient;

/// Builds the first review page URL from checked repository and thread identity.
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

/// Builds the initial 100-item comment URL without fetching or resolving the supplied discussion.
///
/// Repository owner/name and thread number are encoded as endpoint segments. This query performs
/// no repository/thread identity comparison; acquisition checks that relationship before I/O.
pub fn issue_comment_list_url(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
) -> Result<Url, GitHubError> {
    initial_issue_comment_url(client, repository, thread)
}

/// Builds the first issue-comment page URL for a selected discussion.
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

/// Builds the first repository thread page for the selected state scope.
///
/// Uses current repository owner/name at the configured REST base, selecting descending source
/// update order and 100 items per page. `since` becomes a provider RFC 3339 lower bound; it neither
/// reserves archive order nor establishes complete coverage. Continuations should use the returned
/// provider link rather than rebuild this initial URL and lose pagination state.
///
/// This performs no network or archive operation. URL construction and timestamp formatting may
/// fail; the latter is reported as invalid provider data to match the acquisition boundary.
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

/// Builds the initial 100-item discussion URL across all source states and update times.
///
/// Selects descending source-update order and performs no request or archive operation. The URL
/// uses current repository owner/name, while durable repository identity remains separate.
pub fn thread_list_url(client: &GitHubClient, repository: &Repository) -> Result<Url, GitHubError> {
    initial_thread_list_url(client, repository, ThreadListState::All, None)
}

/// Builds the initial discussion URL for source state and optional RFC 3339 update lower bound.
///
/// Includes issues and pull requests, with descending update order and 100-item pages. `since` is
/// provider query context rather than an archive acquisition sequence or completeness claim.
/// Returns typed URL/timestamp errors without issuing a request.
pub fn thread_list_url_in_scope(
    client: &GitHubClient,
    repository: &Repository,
    state: ThreadListState,
    since: Option<UtcTimestamp>,
) -> Result<Url, GitHubError> {
    initial_thread_list_url(client, repository, state, since)
}
