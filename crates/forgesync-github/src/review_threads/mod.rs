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

use std::collections::HashSet;

mod normalize;
mod wire;

use forgesync_core::content::{Repository, ReviewThread};
use forgesync_core::identity::{CommitSha, ProviderId, ThreadId};
use normalize::normalize_review_thread;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::review_threads::wire::{
    GraphqlEnvelope, GraphqlPageInfo, GraphqlReviewThread, ReviewThreadCommentsData,
    ReviewThreadCommentsVariables, ReviewThreadsData, ReviewThreadsVariables,
};
use crate::transport::GitHubClient;

const REVIEW_THREADS_QUERY: &str = r#"
query($owner: String!, $repo: String!, $number: Int!, $cursor: String) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      reviewThreads(first: 100, after: $cursor) {
        nodes {
          id isResolved isOutdated path line startLine
          viewerCanResolve viewerCanUnresolve viewerCanReply
          comments(first: 100) {
            nodes {
              id databaseId body
              author { login __typename }
              path diffHunk createdAt updatedAt url
              pullRequestReview { id }
            }
            pageInfo { hasNextPage endCursor }
          }
        }
        pageInfo { hasNextPage endCursor }
      }
    }
  }
}
"#;

const REVIEW_THREAD_COMMENTS_QUERY: &str = r#"
query($threadID: ID!, $cursor: String) {
  node(id: $threadID) {
    ... on PullRequestReviewThread {
      comments(first: 100, after: $cursor) {
        nodes {
          id databaseId body
          author { login __typename }
          path diffHunk createdAt updatedAt url
          pullRequestReview { id }
        }
        pageInfo { hasNextPage endCursor }
      }
    }
  }
}
"#;

/// Opaque GraphQL cursor for one page of review threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphqlCursor(String);

impl GraphqlCursor {
    /// Returns the provider cursor value for use in a later page request.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One typed GraphQL page with every nested comment connection fully acquired.
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
pub async fn fetch_review_thread_page(
    client: &GitHubClient,
    repository: &Repository,
    thread: &ThreadId,
    head_sha: &CommitSha,
    after: Option<&GraphqlCursor>,
    cancellation: &CancellationToken,
) -> Result<GraphqlReviewThreadPage, GitHubError> {
    validate_scope(repository, thread)?;
    let variables = ReviewThreadsVariables {
        owner: &repository.owner,
        repo: &repository.name,
        number: thread.number().get(),
        cursor: after.map(GraphqlCursor::as_str),
    };
    let response: GraphqlEnvelope<ReviewThreadsData> =
        execute_graphql(client, REVIEW_THREADS_QUERY, &variables, cancellation).await?;
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

/// Rejects a review-thread request for a thread outside the selected repository.
fn validate_scope(repository: &Repository, thread: &ThreadId) -> Result<(), GitHubError> {
    if thread.repository() != &repository.id {
        return Err(GitHubError::InvalidProviderData);
    }
    Ok(())
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
    let mut comments = node
        .comments
        .take()
        .ok_or(GitHubError::InvalidProviderData)?;
    let mut comment_nodes = comments
        .nodes
        .take()
        .ok_or(GitHubError::InvalidProviderData)?;
    let mut comment_page_info = comments
        .page_info
        .take()
        .ok_or(GitHubError::InvalidProviderData)?;
    let mut seen_cursors = HashSet::new();
    while has_next_page(&comment_page_info)? {
        let cursor = required_next_cursor(&comment_page_info)?;
        if !seen_cursors.insert(cursor.clone()) {
            return Err(GitHubError::InvalidPaginationLink);
        }
        let variables = ReviewThreadCommentsVariables {
            thread_id: &provider_id,
            cursor: Some(&cursor),
        };
        let response: GraphqlEnvelope<ReviewThreadCommentsData> = execute_graphql(
            client,
            REVIEW_THREAD_COMMENTS_QUERY,
            &variables,
            cancellation,
        )
        .await?;
        let data = response.data.ok_or(GitHubError::InvalidProviderData)?;
        let node_data = data.node.ok_or(GitHubError::InvalidProviderData)?;
        let connection = node_data.comments.ok_or(GitHubError::InvalidProviderData)?;
        comment_nodes.extend(connection.nodes.ok_or(GitHubError::InvalidProviderData)?);
        comment_page_info = connection
            .page_info
            .ok_or(GitHubError::InvalidProviderData)?;
    }

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

/// Requires a usable cursor whenever another GraphQL page is declared.
fn required_next_cursor(page_info: &GraphqlPageInfo) -> Result<String, GitHubError> {
    page_info
        .end_cursor
        .as_deref()
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_owned)
        .ok_or(GitHubError::InvalidPaginationLink)
}

/// Sends one bounded GraphQL request through the configured transport client.
async fn execute_graphql<T, V>(
    client: &GitHubClient,
    query: &str,
    variables: &V,
    cancellation: &CancellationToken,
) -> Result<GraphqlEnvelope<T>, GitHubError>
where
    T: for<'de> Deserialize<'de>,
    V: Serialize,
{
    let url = client.graphql_endpoint_url()?;
    let body = serde_json::to_vec(&GraphqlRequest { query, variables })
        .map_err(|_| GitHubError::InvalidProviderData)?;
    let response: GraphqlEnvelope<T> = client.post_json(&url, &body, cancellation).await?;
    if !response.errors.is_empty() {
        return Err(GitHubError::GraphqlErrors {
            count: u32::try_from(response.errors.len()).unwrap_or(u32::MAX),
        });
    }
    Ok(response)
}

#[derive(Serialize)]
struct GraphqlRequest<'a, V> {
    query: &'a str,
    variables: &'a V,
}

#[cfg(test)]
mod tests;
