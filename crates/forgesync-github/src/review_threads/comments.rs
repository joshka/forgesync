//! # Complete one review thread's nested comment connection
//!
//! `CommentPages` owns accumulated raw comments, current page metadata, and consumed cursors for
//! one provider review-thread ID. Construction checks the initial connection; consuming completion
//! acquires subsequent pages before returning the ordered nodes for normalization.
//!
//! Each continuation requires an explicit provider flag and nonempty cursor. Reusing a consumed
//! cursor fails before another request, preventing endless nested traversal. The initial and later
//! page members remain in provider order; no partial node list escapes on request or metadata
//! failure.
//!
//! GraphQL request execution owns transport and partial-error rejection. This owner validates the
//! nested data/node/connection and retains the selected thread ID across pages. Domain
//! normalization and outer-page completeness remain with their callers; no archive cursor or
//! membership is written.

use std::collections::HashSet;

use forgesync_core::identity::ProviderId;
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::review_threads::request::{GraphqlRequest, REVIEW_THREAD_COMMENTS_QUERY};
use crate::review_threads::wire::{
    GraphqlComment, GraphqlConnection, GraphqlEnvelope, GraphqlPageInfo, ReviewThreadCommentsData,
    ReviewThreadCommentsVariables,
};
use crate::transport::GitHubClient;

/// Pending nested connection whose partial members stay private until completion succeeds.
pub struct CommentPages {
    /// Checked provider node ID used for every subsequent nested query.
    provider_id: ProviderId,
    /// Accumulated raw nodes in initial-page then continuation order.
    nodes: Vec<GraphqlComment>,
    /// Current provider continuation claim, checked before issuing the next request.
    page_info: GraphqlPageInfo,
    /// Cursors already used for continuation requests within this one connection.
    seen_cursors: HashSet<String>,
}

impl CommentPages {
    /// Checks initial member/page-info presence without claiming that further pages completed.
    pub fn new(
        provider_id: ProviderId,
        connection: GraphqlConnection<GraphqlComment>,
    ) -> Result<Self, GitHubError> {
        let nodes = connection.nodes.ok_or(GitHubError::InvalidProviderData)?;
        let page_info = connection
            .page_info
            .ok_or(GitHubError::InvalidProviderData)?;
        Ok(Self {
            provider_id,
            nodes,
            page_info,
            seen_cursors: HashSet::new(),
        })
    }

    /// Consumes every declared continuation or returns an error without exposing partial members.
    pub async fn complete(
        mut self,
        client: &GitHubClient,
        cancellation: &CancellationToken,
    ) -> Result<Vec<GraphqlComment>, GitHubError> {
        while let Some(cursor) = self.next_cursor()? {
            let response = self.read_page(client, &cursor, cancellation).await?;
            self.apply_page(response)?;
        }
        Ok(self.nodes)
    }

    /// Requires a usable unseen cursor only when another page is explicitly declared.
    fn next_cursor(&mut self) -> Result<Option<String>, GitHubError> {
        if !self
            .page_info
            .has_next_page
            .ok_or(GitHubError::InvalidProviderData)?
        {
            return Ok(None);
        }
        let cursor = self
            .page_info
            .end_cursor
            .as_deref()
            .filter(|cursor| !cursor.is_empty())
            .ok_or(GitHubError::InvalidPaginationLink)?
            .to_owned();
        if !self.seen_cursors.insert(cursor.clone()) {
            return Err(GitHubError::InvalidPaginationLink);
        }
        Ok(Some(cursor))
    }

    /// Requests the next comment page with the same provider node and caller cancellation scope.
    async fn read_page(
        &self,
        client: &GitHubClient,
        cursor: &str,
        cancellation: &CancellationToken,
    ) -> Result<GraphqlEnvelope<ReviewThreadCommentsData>, GitHubError> {
        let variables = ReviewThreadCommentsVariables {
            thread_id: &self.provider_id,
            cursor: Some(cursor),
        };
        let request = GraphqlRequest {
            query: REVIEW_THREAD_COMMENTS_QUERY,
            variables: &variables,
        };
        request.execute(client, cancellation).await
    }

    /// Validates nested response structure and advances accumulated nodes and continuation
    /// together.
    fn apply_page(
        &mut self,
        response: GraphqlEnvelope<ReviewThreadCommentsData>,
    ) -> Result<(), GitHubError> {
        let data = response.data.ok_or(GitHubError::InvalidProviderData)?;
        let node = data.node.ok_or(GitHubError::InvalidProviderData)?;
        let connection = node.comments.ok_or(GitHubError::InvalidProviderData)?;
        let nodes = connection.nodes.ok_or(GitHubError::InvalidProviderData)?;
        let page_info = connection
            .page_info
            .ok_or(GitHubError::InvalidProviderData)?;
        self.nodes.extend(nodes);
        self.page_info = page_info;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Cursor policy can be checked without transport or hidden pagination scenarios.
    //!
    //! These cases construct empty raw connections explicitly. Member acquisition remains covered
    //! by provider request scenarios; here only terminal, missing, and repeated continuation claims
    //! determine whether another request is permitted.

    use forgesync_core::identity::ProviderId;

    use crate::error::GitHubError;
    use crate::review_threads::comments::CommentPages;
    use crate::review_threads::wire::{GraphqlConnection, GraphqlPageInfo};

    #[test]
    fn repeated_cursor_is_rejected_before_a_second_request() {
        let connection = GraphqlConnection {
            nodes: Some(Vec::new()),
            page_info: Some(GraphqlPageInfo {
                has_next_page: Some(true),
                end_cursor: Some("next".to_owned()),
            }),
        };
        let mut pages = CommentPages::new(ProviderId::new("thread").expect("node ID"), connection)
            .expect("initial connection");
        assert_eq!(
            pages.next_cursor().expect("first cursor"),
            Some("next".to_owned())
        );
        assert!(matches!(
            pages.next_cursor(),
            Err(GitHubError::InvalidPaginationLink)
        ));
    }

    #[test]
    fn terminal_page_needs_no_cursor() {
        let connection = GraphqlConnection {
            nodes: Some(Vec::new()),
            page_info: Some(GraphqlPageInfo {
                has_next_page: Some(false),
                end_cursor: None,
            }),
        };
        let mut pages = CommentPages::new(ProviderId::new("thread").expect("node ID"), connection)
            .expect("initial connection");
        assert_eq!(pages.next_cursor().expect("terminal page"), None);
    }

    #[test]
    fn missing_flag_is_invalid_provider_data() {
        let connection = GraphqlConnection {
            nodes: Some(Vec::new()),
            page_info: Some(GraphqlPageInfo {
                has_next_page: None,
                end_cursor: None,
            }),
        };
        let mut pages = CommentPages::new(ProviderId::new("thread").expect("node ID"), connection)
            .expect("initial connection");
        assert!(matches!(
            pages.next_cursor(),
            Err(GitHubError::InvalidProviderData)
        ));
    }
}
