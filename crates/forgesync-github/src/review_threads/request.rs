//! GraphQL operation text and execution for review threads.

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::review_threads::wire::GraphqlEnvelope;
use crate::transport::GitHubClient;

/// Outer thread page with its first nested comment page.
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

/// Further comments for one review-thread node; keep its comment fields in sync with the outer
/// query.
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

#[derive(Serialize)]
pub struct GraphqlRequest<'a, V> {
    pub query: &'a str,
    pub variables: &'a V,
}

impl<V: Serialize> GraphqlRequest<'_, V> {
    /// Executes the operation and rejects any envelope with errors, even alongside partial data,
    /// so partial GraphQL success is never mistaken for complete membership.
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
