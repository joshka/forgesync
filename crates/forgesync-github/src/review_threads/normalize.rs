//! Convert fully acquired GraphQL review threads into domain evidence.
//!
//! The parent module completes nested comment pagination before calling this converter. It then
//! checks provider IDs, line and file context, thread state, and comment identity before
//! constructing `forgesync-core::content::ReviewThread` values.
//!
//! Normalization does not fetch another page or write the archive. It assumes that the caller has
//! delivered the complete comment connection for one review thread; a partially paged connection
//! must remain an acquisition failure, not a smaller apparently complete thread.
//!
//! Use this module when tracing a GraphQL field into normalized review evidence. The engine ties
//! the result to a pull-request head, and the store decides whether that head-bound family can
//! replace current membership.

use super::{
    Comment, CommentId, CommitSha, GitHubError, GraphqlComment, GraphqlReviewThread, ProviderData,
    ProviderId, ReviewId, ReviewThread, ReviewThreadId, ThreadId, UtcTimestamp, Value,
};

/// Converts a fully paged GraphQL thread into normalized review evidence.
pub fn normalize_review_thread(
    thread: &ThreadId,
    head_sha: &CommitSha,
    provider_id: ProviderId,
    node: GraphqlReviewThread,
    comment_nodes: Vec<GraphqlComment>,
) -> Result<ReviewThread, GitHubError> {
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

/// Converts a GraphQL review comment without losing provider identity.
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
