//! GraphQL review-thread acquisition with nested comment pagination.
//!
//! A returned outer page has every nested comment connection fully paged, so callers can stage
//! its threads without persisting partial nested state. Completing the outer family remains an
//! engine/store decision.

mod comments;
mod normalize;
mod request;
mod wire;

use forgesync_core::content::{Repository, ReviewThread};
use forgesync_core::identity::{CommitSha, ProviderId, ThreadId};
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::resources::require_thread_scope;
use crate::review_threads::comments::complete_comments;
use crate::review_threads::normalize::normalize_review_thread;
use crate::review_threads::request::{GraphqlRequest, REVIEW_THREADS_QUERY};
use crate::review_threads::wire::{
    GraphqlEnvelope, GraphqlPageInfo, GraphqlReviewThread, ReviewThreadsData,
    ReviewThreadsVariables,
};
use crate::transport::GitHubClient;

/// Opaque provider continuation for the outer review-thread connection.
///
/// Keep it paired with the repository and pull request that produced it; it does not encode scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphqlCursor(String);

impl GraphqlCursor {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One outer page of review threads, each with its complete comment list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphqlReviewThreadPage {
    pub review_threads: Vec<ReviewThread>,
    pub next_cursor: Option<GraphqlCursor>,
}

/// Fetches one page of current review-thread state, fully paging nested comments.
///
/// `repository` supplies the GraphQL owner/name and `thread` the pull-request number; their
/// repository identities must agree. `head_sha` labels the evidence with the caller's selected
/// head and is not verified against GitHub.
///
/// # Errors
///
/// Any GraphQL error entry (even with partial data), malformed payload, invalid continuation, or
/// nested-page failure rejects the whole page; no partial members are returned.
pub async fn fetch_review_thread_page(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
    head_sha: &CommitSha,
    after: Option<&GraphqlCursor>,
    cancellation: &CancellationToken,
) -> Result<GraphqlReviewThreadPage, GitHubError> {
    require_thread_scope(repository, thread)?;
    let variables = ReviewThreadsVariables {
        owner: &repository.owner,
        repo: &repository.name,
        number: thread.number().get(),
        cursor: after.map(GraphqlCursor::as_str),
    };
    let request = GraphqlRequest {
        query: REVIEW_THREADS_QUERY,
        variables: &variables,
    };
    let response: GraphqlEnvelope<ReviewThreadsData> =
        request.execute(client, cancellation).await?;
    let connection = response
        .data
        .and_then(|data| data.repository)
        .and_then(|repository| repository.pull_request)
        .ok_or(GitHubError::InvalidProviderData)?
        .review_threads;
    let mut review_threads = Vec::with_capacity(connection.nodes.len());
    for node in connection.nodes {
        review_threads
            .push(complete_review_thread(client, thread, head_sha, node, cancellation).await?);
    }
    let next_cursor =
        next_cursor(&connection.page_info)?.map(|cursor| GraphqlCursor(cursor.to_owned()));
    Ok(GraphqlReviewThreadPage {
        review_threads,
        next_cursor,
    })
}

async fn complete_review_thread(
    client: &GitHubClient,
    thread: &ThreadId,
    head_sha: &CommitSha,
    mut node: GraphqlReviewThread,
    cancellation: &CancellationToken,
) -> Result<ReviewThread, GitHubError> {
    let provider_id =
        ProviderId::new(node.id.clone()).map_err(|_| GitHubError::InvalidProviderData)?;
    complete_comments(client, &provider_id, &mut node.comments, cancellation).await?;
    normalize_review_thread(thread, head_sha, provider_id, node)
}

/// Requires a nonempty cursor when GraphQL says another page exists; otherwise pagination could
/// silently stop with missing members.
fn next_cursor(page_info: &GraphqlPageInfo) -> Result<Option<&str>, GitHubError> {
    if !page_info.has_next_page {
        return Ok(None);
    }
    match page_info.end_cursor.as_deref() {
        Some(cursor) if !cursor.is_empty() => Ok(Some(cursor)),
        _ => Err(GitHubError::InvalidPaginationLink),
    }
}

#[cfg(test)]
mod tests;
