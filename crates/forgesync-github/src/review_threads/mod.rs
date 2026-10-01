//! GraphQL review-thread acquisition with nested comment pagination.
//!
//! [`fetch_review_thread_page`] fetches one outer review-thread page. A thread on that page can
//! itself have additional comment pages; this module finishes those nested connections before
//! returning a normalized [`GraphqlReviewThreadPage`]. [`GraphqlCursor`] holds the outer
//! continuation position.
//!
//! The caller supplies the repository, pull request, and current head SHA. The scope check
//! prevents a thread from being attached to another repository. Cursor validation rejects a
//! provider claim of more pages without a usable next cursor, and repeated nested cursors are
//! treated as invalid pagination rather than an infinite loop.
//!
//! The engine records review threads as a head-bound evidence family. Returning one complete outer
//! page does not mean the family is complete until every outer page has been acquired and applied.
//! Transport owns the HTTP request; `normalize` owns GraphQL-to-domain conversion.

mod comments;
mod normalize;
mod request;
mod wire;

use forgesync_core::content::{Repository, ReviewThread};
use forgesync_core::identity::{CommitSha, ProviderId, ThreadId};
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::resources::require_thread_scope;
use crate::review_threads::comments::CommentPages;
use crate::review_threads::normalize::normalize_review_thread;
use crate::review_threads::request::{GraphqlRequest, REVIEW_THREADS_QUERY};
use crate::review_threads::wire::{
    GraphqlEnvelope, GraphqlPageInfo, GraphqlReviewThread, ReviewThreadsData,
    ReviewThreadsVariables,
};
use crate::transport::GitHubClient;

/// Opaque continuation returned by a successful review-thread page request.
///
/// Keep this value paired with the same repository and pull request when requesting another page.
/// It preserves provider spelling and proves only that a nonempty continuation was supplied; it
/// does not encode or independently validate the resource scope. Start acquisition with `None`
/// rather than constructing a cursor from unrelated provider data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphqlCursor(
    /// Provider-issued spelling retained unchanged for the next outer request.
    String,
);

impl GraphqlCursor {
    /// Borrows the provider spelling without decoding or rewriting it.
    ///
    /// This is useful for diagnostics or request encoding. The spelling alone does not identify
    /// the pull request to which the continuation belongs.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One outer page whose review threads have fully acquired nested comment connections.
///
/// Preserve the returned order when staging observations. An empty vector is a successful empty
/// page, whereas acquisition or normalization failure returns an error without this value. A
/// terminal page completes the outer traversal only when all preceding pages also succeeded;
/// this type does not grant archive replacement authority by itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphqlReviewThreadPage {
    /// Review threads with complete nested comment lists.
    pub review_threads: Vec<ReviewThread>,
    /// Cursor for the next outer review-thread page, when present.
    pub next_cursor: Option<GraphqlCursor>,
}

/// Fetches one page of current review-thread state, fully paging nested comments.
///
/// The returned page is only successful when the outer connection and every nested comments
/// connection have valid page metadata and all GraphQL responses have no partial errors. Callers
/// can therefore stage its threads as one complete page without persisting partial nested state.
///
/// `repository` supplies the display owner/name used by GraphQL, and `thread` supplies the local
/// pull-request number. Their stable repository identities must agree. The caller must select a
/// client for the repository host and a pull-request thread: identity equality does not prove
/// host routing, provider existence, or thread kind. Keep `after` paired with this same resource.
///
/// `head_sha` labels the normalized evidence with the head selected by the caller. This query does
/// not fetch or verify that head, so the workflow owns detecting head changes and deciding whether
/// the evidence can be applied. Acquisition writes no archive state.
///
/// # Errors
///
/// Returns [`GitHubError::InvalidProviderData`] for a repository identity mismatch, missing
/// required response data, or invalid normalized values. Invalid continuation metadata returns the
/// relevant pagination error. Transport, retry exhaustion, cancellation, and GraphQL envelope
/// failures retain their typed errors. Failure of any nested connection rejects the entire outer
/// page; already fetched members are not returned as a partial success.
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
    let data = response.data.ok_or(GitHubError::InvalidProviderData)?;
    let pull_request = data
        .repository
        .and_then(|repository| repository.pull_request)
        .ok_or(GitHubError::InvalidProviderData)?;
    let connection = pull_request
        .review_threads
        .ok_or(GitHubError::InvalidProviderData)?;
    let page_info = connection
        .page_info
        .ok_or(GitHubError::InvalidProviderData)?;
    let nodes = connection.nodes.ok_or(GitHubError::InvalidProviderData)?;
    let mut review_threads = Vec::with_capacity(nodes.len());
    for node in nodes {
        review_threads
            .push(complete_review_thread(client, thread, head_sha, node, cancellation).await?);
    }
    let next_cursor = cursor_from_page_info(page_info)?;
    Ok(GraphqlReviewThreadPage {
        review_threads,
        next_cursor,
    })
}

/// Finishes the nested comment connection before normalization. A partially paged review thread
/// must not be presented downstream as a complete collection.
async fn complete_review_thread(
    client: &GitHubClient,
    thread: &ThreadId,
    head_sha: &CommitSha,
    mut node: GraphqlReviewThread,
    cancellation: &CancellationToken,
) -> Result<ReviewThread, GitHubError> {
    let provider_id =
        ProviderId::new(node.id.clone()).map_err(|_| GitHubError::InvalidProviderData)?;
    let connection = node
        .comments
        .take()
        .ok_or(GitHubError::InvalidProviderData)?;
    let pages = CommentPages::new(provider_id.clone(), connection)?;
    let comment_nodes = pages.complete(client, cancellation).await?;

    normalize_review_thread(thread, head_sha, provider_id, node, comment_nodes)
}

/// Requires a nonempty cursor when GraphQL says another page exists; otherwise pagination could
/// silently stop with missing review threads.
fn cursor_from_page_info(page_info: GraphqlPageInfo) -> Result<Option<GraphqlCursor>, GitHubError> {
    let has_next_page = has_next_page(&page_info)?;
    match (has_next_page, page_info.end_cursor) {
        (true, Some(cursor)) if !cursor.is_empty() => Ok(Some(GraphqlCursor(cursor))),
        (true, _) => Err(GitHubError::InvalidPaginationLink),
        (false, _) => Ok(None),
    }
}

/// Reads the provider's pagination flag while rejecting missing page metadata.
fn has_next_page(page_info: &GraphqlPageInfo) -> Result<bool, GitHubError> {
    page_info
        .has_next_page
        .ok_or(GitHubError::InvalidProviderData)
}

#[cfg(test)]
mod tests;
