//! Reserved child-family collections for one discussion.
//!
//! Comments, pull-request metadata, reviews, and review threads share one archive protocol:
//! reserve an observation, stage provider pages, then finish complete or incomplete. Only the
//! per-family page traversal differs. Staged pages never change canonical membership until a
//! complete finish is applied; an incomplete finish preserves prior complete membership.
//! Cancellation records incomplete coverage without a provider-failure ledger entry.

use std::collections::HashSet;

use forgesync_core::content::{Discussion, PullRequestMetadata, Repository};
use forgesync_core::coverage::{EvidenceFamily, Failure, FailureKind};
use forgesync_core::identity::{CommitSha, ObservationSequence, ThreadId};
use forgesync_core::observation::{CollectionCompleteness, IncompleteReason, SourceClock};
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_github::error::GitHubError;
use forgesync_github::resources::{
    fetch_issue_comment_page, fetch_pull_request_metadata, fetch_pull_request_review_page,
};
use forgesync_github::review_threads::fetch_review_thread_page;
use forgesync_github::transport::GitHubClient;
use forgesync_store::error::StoreError;
use forgesync_store::families::{ChildFamilyObservation, ChildFamilyPage, ChildFamilyRequest};
use forgesync_store::observations::{ObservationDisposition, StagedItem};
use forgesync_store::runs::{ChildFamilyFailureScope, RunFailureInput};
use serde::Serialize;

use crate::clock::now_utc;
use crate::error::EngineError;
use crate::provider_failure::github_failure;
use crate::sync::accounting::FamilyResult;
use crate::sync::coordinator::Run;

/// One discussion whose child families are acquired within a run's thread-state scope.
pub struct ThreadTarget<'a> {
    pub run: &'a Run<'a>,
    pub client: &'a GitHubClient,
    pub repository: &'a Repository,
    /// Ledger scope key of the enclosing thread-state unit.
    pub scope_key: &'static str,
    pub thread: &'a ThreadId,
    /// Parent source time; it attributes every family observation of this discussion.
    pub updated_at: UtcTimestamp,
}

impl ThreadTarget<'_> {
    /// Skips current comment evidence or collects every REST comment page.
    pub async fn comments(&self, discussion: &Discussion) -> Result<FamilyResult, EngineError> {
        let source_count = discussion
            .provider_data
            .get("comments")
            .and_then(serde_json::Value::as_u64);
        let current = self
            .run
            .archive
            .child_family_is_current(
                self.thread,
                EvidenceFamily::Comments,
                &self.clock(),
                source_count,
            )
            .await?;
        if current {
            self.resolve_failures(EvidenceFamily::Comments).await?;
            return Ok(FamilyResult::default());
        }
        let mut collection =
            FamilyCollection::reserve(self, EvidenceFamily::Comments, None).await?;
        let mut next = None;
        loop {
            let page = fetch_issue_comment_page(
                self.client,
                self.repository,
                self.thread,
                next.as_ref(),
                self.run.cancellation,
            )
            .await;
            let page = match page {
                Ok(page) => page,
                Err(error) => return collection.fail(error).await,
            };
            collection
                .stage(page.items, |comment| comment.id.provider_id().clone())
                .await?;
            next = page.next_page;
            if next.is_none() {
                return collection.complete().await;
            }
        }
    }

    /// Acquires head metadata; the head is returned only after canonical application.
    pub async fn metadata(
        &self,
    ) -> Result<(FamilyResult, Option<PullRequestMetadata>), EngineError> {
        let mut collection =
            FamilyCollection::reserve(self, EvidenceFamily::PullRequestMetadata, None).await?;
        let fetched = fetch_pull_request_metadata(
            self.client,
            self.repository,
            self.thread,
            self.run.cancellation,
        )
        .await;
        let metadata = match fetched {
            Ok(metadata) => metadata,
            Err(error) => return Ok((collection.fail(error).await?, None)),
        };
        collection
            .stage(vec![metadata.clone()], |_| {
                self.thread.provider_id().clone()
            })
            .await?;
        Ok((collection.complete().await?, Some(metadata)))
    }

    /// Acquires a head-bound review family after the metadata attempt for this discussion.
    ///
    /// Current evidence for the same source clock and head resolves old failures without a
    /// request. Missing metadata still reserves and finishes an incomplete observation with a
    /// scoped failure, so the skipped acquisition is inspectable.
    pub async fn review_family(
        &self,
        family: EvidenceFamily,
        metadata: &(FamilyResult, Option<PullRequestMetadata>),
    ) -> Result<FamilyResult, EngineError> {
        let (metadata_result, metadata) = metadata;
        if let Some(metadata) = metadata {
            let current = self
                .run
                .archive
                .pull_request_family_is_current_for_head(
                    self.thread,
                    family,
                    &self.clock(),
                    &metadata.head.sha,
                )
                .await?;
            if current {
                self.resolve_failures(family).await?;
                return Ok(FamilyResult::default());
            }
        }
        let head = metadata.as_ref().map(|metadata| metadata.head.sha.clone());
        let collection = FamilyCollection::reserve(self, family, head.clone()).await?;
        let Some(head) = head else {
            let failure = metadata_result.failure.clone().unwrap_or(Failure {
                kind: FailureKind::ProviderResponse,
                message: "pull-request head metadata is unavailable".to_owned(),
            });
            return collection
                .fail_with(failure, IncompleteReason::Unknown)
                .await;
        };
        match family {
            EvidenceFamily::ReviewThreads => collection.review_thread_pages(&head).await,
            _ => collection.review_pages().await,
        }
    }

    /// Source clock attributed to every family observation of this discussion.
    fn clock(&self) -> SourceClock {
        SourceClock::Valid(self.updated_at)
    }

    /// Ledger scope for this discussion's failures in `family`.
    fn failure_scope(&self, family: EvidenceFamily) -> ChildFamilyFailureScope<'_> {
        ChildFamilyFailureScope {
            run_id: self.run.id,
            repository: &self.repository.id,
            thread: self.thread,
            family,
            scope_key: self.scope_key,
        }
    }

    /// Clears earlier failures once current or newly completed evidence satisfies the family.
    async fn resolve_failures(&self, family: EvidenceFamily) -> Result<(), EngineError> {
        self.run
            .archive
            .resolve_child_family_failures(self.run.lease, &self.failure_scope(family), now_utc()?)
            .await?;
        Ok(())
    }
}

/// A reserved observation and its provisional page progress until a consuming finish.
struct FamilyCollection<'a> {
    target: &'a ThreadTarget<'a>,
    family: EvidenceFamily,
    /// Pull-request head the complete observation is bound to, for head-aware families.
    head: Option<CommitSha>,
    sequence: ObservationSequence,
    /// Staged pages: also the next zero-based page index and the expected page count.
    pages: u32,
    /// Staged items, which can repeat identities and so are not canonical membership size.
    received: u64,
}

impl<'a> FamilyCollection<'a> {
    /// Reserves local order and marks earlier failures of this family as retried.
    async fn reserve(
        target: &'a ThreadTarget<'a>,
        family: EvidenceFamily,
        head: Option<CommitSha>,
    ) -> Result<Self, EngineError> {
        let run = target.run;
        let request_scope = format!("run:{}:{}", run.id.get(), target.scope_key);
        let reservation = run
            .archive
            .reserve_child_family_observation_fenced(
                ChildFamilyRequest {
                    thread: target.thread,
                    family,
                    source_clock: &target.clock(),
                    started_at: now_utc()?,
                    request_scope: &request_scope,
                },
                run.lease,
            )
            .await?;
        if !reservation.reserved {
            return Err(StoreError::StaleObservationGeneration.into());
        }
        run.archive
            .mark_child_family_failures_retried(run.lease, &target.failure_scope(family))
            .await?;
        Ok(Self {
            target,
            family,
            head,
            sequence: reservation.sequence,
            pages: 0,
            received: 0,
        })
    }

    /// Stages one provider page without asserting complete canonical membership.
    async fn stage<T: Serialize>(
        &mut self,
        items: Vec<T>,
        id: impl Fn(&T) -> forgesync_core::identity::ProviderId,
    ) -> Result<(), EngineError> {
        let items = items
            .into_iter()
            .map(|payload| StagedItem {
                id: id(&payload),
                payload,
            })
            .collect::<Vec<_>>();
        let run = self.target.run;
        run.archive
            .stage_child_family_page_fenced(
                ChildFamilyPage {
                    thread: self.target.thread,
                    family: self.family,
                    sequence: self.sequence,
                    page_index: self.pages,
                    items: &items,
                },
                run.lease,
            )
            .await?;
        self.received += items.len() as u64;
        self.pages += 1;
        Ok(())
    }

    /// Ends acquisition after a provider error: cancellation interrupts, others are recorded.
    async fn fail(self, error: GitHubError) -> Result<FamilyResult, EngineError> {
        let reason = match error {
            GitHubError::Cancelled => IncompleteReason::Cancelled,
            GitHubError::Deferred { .. } => IncompleteReason::RetryBudget,
            _ if self.pages > 0 => IncompleteReason::Pagination,
            _ => IncompleteReason::Unknown,
        };
        if matches!(error, GitHubError::Cancelled) {
            self.finish_incomplete(reason).await?;
            return Ok(FamilyResult {
                interrupted: true,
                ..self.result()
            });
        }
        self.fail_with(github_failure(&error), reason).await
    }

    /// Finishes incomplete coverage and records `failure` in the run ledger.
    async fn fail_with(
        self,
        failure: Failure,
        reason: IncompleteReason,
    ) -> Result<FamilyResult, EngineError> {
        self.finish_incomplete(reason).await?;
        let target = self.target;
        target
            .run
            .record_failure(RunFailureInput {
                run_id: target.run.id,
                target: &target.repository.full_name,
                repository: Some(&target.repository.id),
                thread: Some(target.thread),
                family: Some(self.family),
                scope_key: target.scope_key,
                failure: &failure,
                created_at: now_utc()?,
            })
            .await?;
        Ok(FamilyResult {
            failure: Some(failure),
            ..self.result()
        })
    }

    /// Records incomplete coverage, preserving any previously complete membership.
    async fn finish_incomplete(&self, reason: IncompleteReason) -> Result<(), EngineError> {
        let completeness = CollectionCompleteness::Incomplete {
            reason,
            received_items: self.received,
        };
        self.finish(&completeness, None, None).await?;
        Ok(())
    }

    /// Promotes complete membership; a superseded reservation is an archive error.
    async fn complete(self) -> Result<FamilyResult, EngineError> {
        let observation = self
            .finish(
                &CollectionCompleteness::Complete,
                Some(self.pages),
                self.head.as_ref(),
            )
            .await?;
        if !matches!(
            observation.disposition,
            ObservationDisposition::Applied | ObservationDisposition::Replayed
        ) {
            return Err(StoreError::StaleObservationGeneration.into());
        }
        self.target.resolve_failures(self.family).await?;
        Ok(FamilyResult {
            items_committed: observation.item_count,
            ..self.result()
        })
    }

    /// Applies one terminal coverage state to the reserved observation.
    async fn finish(
        &self,
        completeness: &CollectionCompleteness,
        expected_pages: Option<u32>,
        head_sha: Option<&CommitSha>,
    ) -> Result<forgesync_store::observations::FamilyObservationResult, EngineError> {
        let run = self.target.run;
        let observation = ChildFamilyObservation {
            thread: self.target.thread,
            family: self.family,
            sequence: self.sequence,
            observed_at: now_utc()?,
            completeness,
            expected_pages,
            head_sha,
        };
        Ok(run
            .archive
            .finish_child_family_observation_fenced(observation, run.lease)
            .await?)
    }

    /// Page and received-item counts credited to this attempt so far.
    fn result(&self) -> FamilyResult {
        FamilyResult {
            pages_completed: u64::from(self.pages),
            items_received: self.received,
            ..FamilyResult::default()
        }
    }

    /// Follows REST review page links for the collection's head.
    async fn review_pages(mut self) -> Result<FamilyResult, EngineError> {
        let target = self.target;
        let mut next = None;
        loop {
            let page = fetch_pull_request_review_page(
                target.client,
                target.repository,
                target.thread,
                next.as_ref(),
                target.run.cancellation,
            )
            .await;
            let page = match page {
                Ok(page) => page,
                Err(error) => return self.fail(error).await,
            };
            self.stage(page.items, |review| review.id.provider_id().clone())
                .await?;
            next = page.next_page;
            if next.is_none() {
                return self.complete().await;
            }
        }
    }

    /// Traverses GraphQL review-thread pages, rejecting a repeated cursor before staging its page.
    async fn review_thread_pages(mut self, head: &CommitSha) -> Result<FamilyResult, EngineError> {
        let target = self.target;
        let mut next = None;
        let mut seen = HashSet::new();
        loop {
            let page = fetch_review_thread_page(
                target.client,
                target.repository,
                target.thread,
                head,
                next.as_ref(),
                target.run.cancellation,
            )
            .await;
            let page = match page {
                Ok(page) => page,
                Err(error) => return self.fail(error).await,
            };
            if let Some(cursor) = &page.next_cursor
                && !seen.insert(cursor.as_str().to_owned())
            {
                return self.fail(GitHubError::InvalidPaginationLink).await;
            }
            self.stage(page.review_threads, |thread| {
                thread.id.provider_id().clone()
            })
            .await?;
            next = page.next_cursor;
            if next.is_none() {
                return self.complete().await;
            }
        }
    }
}
