//! Convert fully paged GraphQL review threads into domain evidence.

use forgesync_core::content::{Comment, ReviewThread};
use forgesync_core::identity::{
    CommentId, CommitSha, ProviderId, ReviewId, ReviewThreadId, ThreadId,
};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use serde_json::Value;

use crate::error::GitHubError;
use crate::review_threads::wire::{GraphqlComment, GraphqlReviewThread};

/// Converts a thread whose comment connection has already been fully paged.
///
/// Unknown thread fields and `startLine` are retained as provider data.
pub fn normalize_review_thread(
    thread: &ThreadId,
    head_sha: &CommitSha,
    provider_id: ProviderId,
    node: GraphqlReviewThread,
) -> Result<ReviewThread, GitHubError> {
    let comments = node
        .comments
        .nodes
        .into_iter()
        .map(|comment| normalize_comment(thread, comment))
        .collect::<Result<Vec<_>, _>>()?;
    let line = node
        .line
        .map(|line| u64::try_from(line).map_err(|_| GitHubError::InvalidProviderData))
        .transpose()?;
    let mut provider_data = ProviderData::from(node.extra);
    if let Some(start_line) = node.start_line {
        provider_data.insert("startLine", Value::from(start_line));
    }
    Ok(ReviewThread {
        id: ReviewThreadId::new(thread.clone(), provider_id),
        head_sha: head_sha.clone(),
        is_resolved: node.is_resolved,
        is_outdated: node.is_outdated,
        path: node.path,
        line,
        comments,
        provider_data,
    })
}

/// Converts a GraphQL review comment, keeping its provider identity.
fn normalize_comment(thread: &ThreadId, comment: GraphqlComment) -> Result<Comment, GitHubError> {
    let mut provider_data = ProviderData::from(comment.extra);
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
