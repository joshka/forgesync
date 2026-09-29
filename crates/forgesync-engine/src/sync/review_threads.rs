//! # GraphQL pagination for pull-request review threads
//!
//! `ReviewSync::sync_review_threads` prepares a head-aware family and collects GraphQL pages.
//! Shared `review_collection` code owns reservation, staging counts, failures, and complete or
//! incomplete finalization. This file owns the distinct GraphQL traversal and review-thread
//! member mapping, including nested comments already normalized by the provider adapter.
//!
//! `ReviewThreadPages` retains the next cursor and the set of cursor values seen in this attempt.
//! A repeated continuation cursor fails before the offending page is staged, so an invalid
//! traversal cannot become a complete canonical collection. Provider and pagination errors use
//! the same incomplete terminal path; previous complete membership remains available.

use std::collections::HashSet;

use forgesync_core::content::{PullRequestMetadata, ReviewThread};
use forgesync_github::error::GitHubError;
use forgesync_github::review_threads::{
    GraphqlCursor, GraphqlReviewThreadPage, fetch_review_thread_page,
};
use forgesync_store::observations::StagedItem;

use super::ThreadFamilyResult;
use super::review_collection::{ReviewCollection, ReviewPreparation, ReviewSync};
use crate::error::EngineError;

impl ReviewSync<'_> {
    /// Acquires review threads after preparation resolves freshness and head availability.
    pub async fn sync_review_threads(
        self,
        metadata: &ThreadFamilyResult<PullRequestMetadata>,
    ) -> Result<ThreadFamilyResult<()>, EngineError> {
        match self.prepare(metadata).await? {
            ReviewPreparation::Ready(collection) => collection.collect_review_threads().await,
            ReviewPreparation::Finished(result) => Ok(result),
        }
    }
}

impl ReviewCollection<'_> {
    /// Traverses GraphQL pages, rejecting cursor cycles before staging the affected page.
    async fn collect_review_threads(mut self) -> Result<ThreadFamilyResult<()>, EngineError> {
        let mut pages = ReviewThreadPages::default();
        loop {
            let page = match self.review_thread_page(pages.next.as_ref()).await {
                Ok(page) => page,
                Err(error) => return self.fail(error).await,
            };
            if let Err(error) = pages.advance(page.next_cursor) {
                return self.fail(error).await;
            }
            self.stage_review_threads(page.review_threads).await?;
            if pages.next.is_none() {
                return self.complete().await;
            }
        }
    }

    /// Requests provider evidence for the head captured when this collection was prepared.
    async fn review_thread_page(
        &self,
        cursor: Option<&GraphqlCursor>,
    ) -> Result<GraphqlReviewThreadPage, GitHubError> {
        fetch_review_thread_page(
            self.target.client,
            self.target.scope.repository,
            self.target.scope.thread,
            &self.head,
            cursor,
            self.target.context.cancellation,
        )
        .await
    }

    /// Maps normalized review-thread identity into provisional collection membership.
    async fn stage_review_threads(
        &mut self,
        threads: Vec<ReviewThread>,
    ) -> Result<(), EngineError> {
        let items = threads
            .into_iter()
            .map(|thread| StagedItem {
                id: thread.id.provider_id().clone(),
                payload: thread,
            })
            .collect::<Vec<_>>();
        self.stage(&items).await
    }
}

/// Continuation state for one GraphQL traversal, with cycle detection scoped to the attempt.
#[derive(Default)]
struct ReviewThreadPages {
    next: Option<GraphqlCursor>,
    seen: HashSet<String>,
}

impl ReviewThreadPages {
    /// Accepts a terminal cursor or a new continuation; repeated cursors invalidate the traversal.
    fn advance(&mut self, next: Option<GraphqlCursor>) -> Result<(), GitHubError> {
        if let Some(cursor) = &next
            && !self.seen.insert(cursor.as_str().to_owned())
        {
            return Err(GitHubError::InvalidPaginationLink);
        }
        self.next = next;
        Ok(())
    }
}
