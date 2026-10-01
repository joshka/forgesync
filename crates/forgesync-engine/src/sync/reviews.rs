//! # REST pagination for pull-request reviews
//!
//! `ReviewSync::sync_reviews` starts a head-aware review acquisition. Shared preparation in
//! `review_collection` decides whether the archive already has current evidence and records
//! missing metadata before any page request. A ready collection then owns staging and completion.
//!
//! This file keeps only the REST-specific part: fetching successive review pages and converting
//! each normalized review into a staged member. Page links remain local to the collector; archive
//! observation sequences, counters, failure attribution, and finalization belong to the collection.
//! A provider error consumes the attempt as incomplete, preserving earlier complete membership.

use forgesync_core::content::{PullRequestMetadata, Review};
use forgesync_github::resources::fetch_pull_request_review_page;
use forgesync_store::observations::StagedItem;

use super::review_collection::{ReviewCollection, ReviewPreparation, ReviewSync};
use crate::error::EngineError;
use crate::sync::scope::ThreadFamilyResult;

impl ReviewSync<'_> {
    /// Acquires reviews only after preparation establishes a reserved, head-aware attempt.
    pub async fn sync_reviews(
        self,
        metadata: &ThreadFamilyResult<PullRequestMetadata>,
    ) -> Result<ThreadFamilyResult<()>, EngineError> {
        match self.prepare(metadata).await? {
            ReviewPreparation::Ready(collection) => collection.collect_reviews().await,
            ReviewPreparation::Finished(result) => Ok(result),
        }
    }
}

impl ReviewCollection<'_> {
    /// Follows REST page links until the family completes or a provider request fails.
    async fn collect_reviews(mut self) -> Result<ThreadFamilyResult<()>, EngineError> {
        let mut next_page = None;
        loop {
            let page = match fetch_pull_request_review_page(
                self.target.client,
                self.target.scope.repository,
                self.target.scope.thread,
                next_page.as_ref(),
                self.target.context.cancellation,
            )
            .await
            {
                Ok(page) => page,
                Err(error) => return self.fail(error).await,
            };
            next_page = page.next_page;
            self.stage_reviews(page.items).await?;
            if next_page.is_none() {
                return self.complete().await;
            }
        }
    }

    /// Preserves provider review identity while handing page accounting to the collection.
    async fn stage_reviews(&mut self, reviews: Vec<Review>) -> Result<(), EngineError> {
        let items = reviews
            .into_iter()
            .map(|review| StagedItem {
                id: review.id.provider_id().clone(),
                payload: review,
            })
            .collect::<Vec<_>>();
        self.stage(&items).await
    }
}
