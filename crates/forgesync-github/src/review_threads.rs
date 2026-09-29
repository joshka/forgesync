//! GraphQL review-thread acquisition and nested pagination.

use std::collections::{BTreeMap, HashSet};

use forgesync_core::content::{Comment, Repository, ReviewThread};
use forgesync_core::identity::{
    CommentId, CommitSha, ProviderId, ReviewId, ReviewThreadId, ThreadId,
};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
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

fn validate_scope(repository: &Repository, thread: &ThreadId) -> Result<(), GitHubError> {
    if thread.repository() != &repository.id {
        return Err(GitHubError::InvalidProviderData);
    }
    Ok(())
}

async fn complete_review_thread(
    client: &GitHubClient,
    thread: &ThreadId,
    head_sha: &CommitSha,
    node: GraphqlReviewThread,
    cancellation: &CancellationToken,
) -> Result<ReviewThread, GitHubError> {
    let provider_id = ProviderId::new(node.id).map_err(|_| GitHubError::InvalidProviderData)?;
    let mut comments = node.comments.ok_or(GitHubError::InvalidProviderData)?;
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

    let comments = comment_nodes
        .into_iter()
        .map(|comment| normalize_comment(thread, comment))
        .collect::<Result<Vec<_>, _>>()?;
    let line = node
        .line
        .map(|line| u64::try_from(line).map_err(|_| GitHubError::InvalidProviderData))
        .transpose()?;
    let mut provider_data =
        ProviderData::from_value(Value::Object(node.extra.into_iter().collect()))
            .map_err(|_| GitHubError::InvalidProviderData)?;
    if let Some(start_line) = node.start_line {
        provider_data.insert("startLine", Value::from(start_line));
    }
    Ok(ReviewThread {
        id: ReviewThreadId::new(thread.clone(), provider_id),
        head_sha: head_sha.clone(),
        is_resolved: node.is_resolved.ok_or(GitHubError::InvalidProviderData)?,
        is_outdated: node.is_outdated.ok_or(GitHubError::InvalidProviderData)?,
        path: node.path,
        line,
        comments,
        provider_data,
    })
}

fn normalize_comment(thread: &ThreadId, comment: GraphqlComment) -> Result<Comment, GitHubError> {
    let mut provider_data =
        ProviderData::from_value(Value::Object(comment.extra.into_iter().collect()))
            .map_err(|_| GitHubError::InvalidProviderData)?;
    if let Some(database_id) = comment.database_id {
        provider_data.insert("databaseId", Value::from(database_id));
    }
    if let Some(path) = comment.path {
        provider_data.insert("path", Value::String(path));
    }
    if let Some(diff_hunk) = comment.diff_hunk {
        provider_data.insert("diffHunk", Value::String(diff_hunk));
    }
    if let Some(url) = comment.url {
        provider_data.insert("url", Value::String(url));
    }
    let author = comment.author.and_then(|author| {
        let login = author.login;
        let author_data = Value::Object(author.extra.into_iter().collect());
        provider_data.insert("author", author_data);
        login
    });
    let review_id = comment
        .pull_request_review
        .map(|review| {
            provider_data.insert("pullRequestReview", serde_json::json!({ "id": review.id }));
            ProviderId::new(review.id)
                .map(|id| ReviewId::new(thread.clone(), id))
                .map_err(|_| GitHubError::InvalidProviderData)
        })
        .transpose()?;
    let created_at =
        UtcTimestamp::parse(&comment.created_at).map_err(|_| GitHubError::InvalidProviderData)?;
    let updated_at = comment
        .updated_at
        .map(|value| UtcTimestamp::parse(&value).map_err(|_| GitHubError::InvalidProviderData))
        .transpose()?;
    let comment_id = ProviderId::new(comment.id).map_err(|_| GitHubError::InvalidProviderData)?;
    Ok(Comment {
        id: CommentId::new(thread.clone(), comment_id),
        review_id,
        author,
        body: comment.body,
        created_at,
        updated_at,
        provider_data,
    })
}

fn cursor_from_page_info(page_info: GraphqlPageInfo) -> Result<Option<GraphqlCursor>, GitHubError> {
    let has_next_page = has_next_page(&page_info)?;
    match (has_next_page, page_info.end_cursor) {
        (true, Some(cursor)) if !cursor.is_empty() => Ok(Some(GraphqlCursor(cursor))),
        (true, _) => Err(GitHubError::InvalidPaginationLink),
        (false, _) => Ok(None),
    }
}

fn has_next_page(page_info: &GraphqlPageInfo) -> Result<bool, GitHubError> {
    page_info
        .has_next_page
        .ok_or(GitHubError::InvalidProviderData)
}

fn required_next_cursor(page_info: &GraphqlPageInfo) -> Result<String, GitHubError> {
    page_info
        .end_cursor
        .as_deref()
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_owned)
        .ok_or(GitHubError::InvalidPaginationLink)
}

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

#[derive(Deserialize)]
struct GraphqlEnvelope<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<Value>,
}

#[derive(Serialize)]
struct ReviewThreadsVariables<'a> {
    owner: &'a str,
    repo: &'a str,
    number: u64,
    cursor: Option<&'a str>,
}

#[derive(Serialize)]
struct ReviewThreadCommentsVariables<'a> {
    #[serde(rename = "threadID")]
    thread_id: &'a ProviderId,
    cursor: Option<&'a str>,
}

#[derive(Deserialize)]
struct ReviewThreadsData {
    repository: Option<GraphqlRepository>,
}

#[derive(Deserialize)]
struct GraphqlRepository {
    #[serde(rename = "pullRequest")]
    pull_request: Option<GraphqlPullRequest>,
}

#[derive(Deserialize)]
struct GraphqlPullRequest {
    #[serde(rename = "reviewThreads")]
    review_threads: Option<GraphqlConnection<GraphqlReviewThread>>,
}

#[derive(Deserialize)]
struct GraphqlConnection<T> {
    nodes: Option<Vec<T>>,
    #[serde(rename = "pageInfo")]
    page_info: Option<GraphqlPageInfo>,
}

#[derive(Deserialize)]
struct GraphqlPageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: Option<bool>,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
struct GraphqlReviewThread {
    id: String,
    #[serde(rename = "isResolved")]
    is_resolved: Option<bool>,
    #[serde(rename = "isOutdated")]
    is_outdated: Option<bool>,
    path: Option<String>,
    line: Option<i64>,
    #[serde(rename = "startLine")]
    start_line: Option<i64>,
    comments: Option<GraphqlConnection<GraphqlComment>>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct GraphqlComment {
    id: String,
    #[serde(rename = "databaseId")]
    database_id: Option<u64>,
    body: String,
    author: Option<GraphqlAuthor>,
    path: Option<String>,
    #[serde(rename = "diffHunk")]
    diff_hunk: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
    url: Option<String>,
    #[serde(rename = "pullRequestReview")]
    pull_request_review: Option<GraphqlReviewRef>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct GraphqlAuthor {
    login: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct GraphqlReviewRef {
    id: String,
}

#[derive(Deserialize)]
struct ReviewThreadCommentsData {
    node: Option<GraphqlReviewThreadNode>,
}

#[derive(Deserialize)]
struct GraphqlReviewThreadNode {
    comments: Option<GraphqlConnection<GraphqlComment>>,
}

#[cfg(test)]
#[path = "review_threads/tests.rs"]
mod tests;
