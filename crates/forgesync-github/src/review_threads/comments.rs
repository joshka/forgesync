//! Complete one review thread's nested comment connection.

use std::collections::HashSet;

use forgesync_core::identity::ProviderId;
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::review_threads::next_cursor;
use crate::review_threads::request::{GraphqlRequest, REVIEW_THREAD_COMMENTS_QUERY};
use crate::review_threads::wire::{
    GraphqlComment, GraphqlConnection, GraphqlEnvelope, ReviewThreadCommentsData,
    ReviewThreadCommentsVariables,
};
use crate::transport::GitHubClient;

/// Appends every remaining comment page to `connection` in provider order.
///
/// A repeated cursor fails before another request so a looping provider cannot stall traversal.
pub async fn complete_comments(
    client: &GitHubClient,
    thread_id: &ProviderId,
    connection: &mut GraphqlConnection<GraphqlComment>,
    cancellation: &CancellationToken,
) -> Result<(), GitHubError> {
    let mut seen_cursors = HashSet::new();
    while let Some(cursor) = next_unseen_cursor(connection, &mut seen_cursors)? {
        let variables = ReviewThreadCommentsVariables {
            thread_id,
            cursor: Some(&cursor),
        };
        let request = GraphqlRequest {
            query: REVIEW_THREAD_COMMENTS_QUERY,
            variables: &variables,
        };
        let response: GraphqlEnvelope<ReviewThreadCommentsData> =
            request.execute(client, cancellation).await?;
        let page = response
            .data
            .and_then(|data| data.node)
            .ok_or(GitHubError::InvalidProviderData)?
            .comments;
        connection.nodes.extend(page.nodes);
        connection.page_info = page.page_info;
    }
    Ok(())
}

fn next_unseen_cursor(
    connection: &GraphqlConnection<GraphqlComment>,
    seen_cursors: &mut HashSet<String>,
) -> Result<Option<String>, GitHubError> {
    let Some(cursor) = next_cursor(&connection.page_info)? else {
        return Ok(None);
    };
    if !seen_cursors.insert(cursor.to_owned()) {
        return Err(GitHubError::InvalidPaginationLink);
    }
    Ok(Some(cursor.to_owned()))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::next_unseen_cursor;
    use crate::error::GitHubError;
    use crate::review_threads::wire::{GraphqlComment, GraphqlConnection, GraphqlPageInfo};

    fn connection(
        has_next_page: bool,
        end_cursor: Option<&str>,
    ) -> GraphqlConnection<GraphqlComment> {
        GraphqlConnection {
            nodes: Vec::new(),
            page_info: GraphqlPageInfo {
                has_next_page,
                end_cursor: end_cursor.map(str::to_owned),
            },
        }
    }

    #[test]
    fn repeated_cursor_is_rejected_before_a_second_request() {
        let connection = connection(true, Some("next"));
        let mut seen = HashSet::new();
        assert_eq!(
            next_unseen_cursor(&connection, &mut seen),
            Ok(Some("next".to_owned()))
        );
        assert_eq!(
            next_unseen_cursor(&connection, &mut seen),
            Err(GitHubError::InvalidPaginationLink)
        );
    }

    #[test]
    fn terminal_page_needs_no_cursor() {
        let connection = connection(false, None);
        assert_eq!(
            next_unseen_cursor(&connection, &mut HashSet::new()),
            Ok(None)
        );
    }
}
