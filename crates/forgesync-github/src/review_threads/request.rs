//! # Send typed review-thread GraphQL operations
//!
//! `GraphqlRequest` pairs operation text and typed variables for the shared GraphQL POST path.
//! Outer review-thread and nested comment queries select their exact provider fields here, beside
//! the serialized request shape. Acquisition remains responsible for connection traversal and
//! required response data; normalization remains responsible for checked domain content.
//!
//! Execution uses the configured transport's origin, body bounds, retry budget, and cancellation.
//! A provider error envelope is rejected even when it contains usable partial data, so callers
//! cannot mistake partial GraphQL success for complete nested membership. Safe errors retain an
//! error count rather than raw provider text. This module writes no archive state.

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::review_threads::wire::GraphqlEnvelope;
use crate::transport::GitHubClient;

/// Outer thread page with its first nested comment page and required source context.
pub const REVIEW_THREADS_QUERY: &str = r#"
query($owner: String!, $repo: String!, $number: Int!, $cursor: String) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      reviewThreads(first: 100, after: $cursor) {
        nodes {
          id isResolved isOutdated path line startLine
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

/// Further comments for one review-thread node, preserving the outer query’s comment fields.
pub const REVIEW_THREAD_COMMENTS_QUERY: &str = r#"
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

/// Borrowed GraphQL operation and variables, serialized together for one bounded request.
#[derive(Serialize)]
pub struct GraphqlRequest<'a, V> {
    /// Provider operation text; it does not select a transport destination.
    pub query: &'a str,
    /// Typed operation coordinates retained in the serialized `variables` object.
    pub variables: &'a V,
}

impl<V: Serialize> GraphqlRequest<'_, V> {
    /// Executes through the configured transport and rejects any partial-error envelope.
    ///
    /// Missing required data remains the acquisition caller's validation responsibility. Encoding,
    /// transport, cancellation, and provider errors return typed failures; provider error messages
    /// are not copied into diagnostics or logs.
    pub async fn execute<T>(
        &self,
        client: &GitHubClient,
        cancellation: &CancellationToken,
    ) -> Result<GraphqlEnvelope<T>, GitHubError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = client.graphql_endpoint_url();
        let body = serde_json::to_vec(self).map_err(|_| GitHubError::InvalidProviderData)?;
        let response: GraphqlEnvelope<T> = client.post_json(&url, &body, cancellation).await?;
        if !response.errors.is_empty() {
            return Err(GitHubError::GraphqlErrors {
                count: u32::try_from(response.errors.len()).unwrap_or(u32::MAX),
            });
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    //! Request encoding keeps operation text and typed coordinates in the two GraphQL fields.

    use crate::review_threads::request::GraphqlRequest;
    use crate::review_threads::wire::ReviewThreadsVariables;

    #[test]
    fn request_serialization_keeps_named_variables_and_null_initial_cursor() {
        let variables = ReviewThreadsVariables {
            owner: "owner",
            repo: "repo",
            number: 17,
            cursor: None,
        };
        let request = GraphqlRequest {
            query: "operation text",
            variables: &variables,
        };
        let json = serde_json::to_value(request).expect("encode request");
        assert_eq!(
            json,
            serde_json::json!({
                "query": "operation text",
                "variables": { "owner": "owner", "repo": "repo", "number": 17, "cursor": null }
            })
        );
    }
}
