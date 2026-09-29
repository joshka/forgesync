//! # Reserved comment acquisition for one discussion
//!
//! `RepositoryWork::collect_comments` checks explicit family freshness before reserving an
//! observation. `CommentCollection` then owns the discussion, reservation sequence, provisional
//! counts, and provider-page traversal until a consuming complete or incomplete terminal write.
//!
//! Source comment count is only part of the freshness check; stored rows alone cannot distinguish
//! an observed empty collection from a failed fetch. Every fetched page is staged before complete
//! membership is promoted. A provider failure or cancellation records incomplete coverage while
//! preserving prior complete members. Thread-level failures remain in the durable ledger for retry.

use forgesync_core::content::{Comment, Discussion};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::ObservationSequence;
use forgesync_core::observation::{CollectionCompleteness, IncompleteReason, SourceClock};
use forgesync_github::error::GitHubError;
use forgesync_github::resources::{RestCommentPage, fetch_issue_comment_page};
use forgesync_store::error::StoreError;
use forgesync_store::families::ChildFamilyObservation;
use forgesync_store::observations::{ObservationDisposition, StagedItem};
use forgesync_store::runs::{ChildFamilyFailureScope, RunFailureInput};
use url::Url;

use super::ThreadFamilyResult;
use super::repository_work::RepositoryWork;
use crate::enumeration::{github_failure, now_utc};
use crate::error::EngineError;

impl RepositoryWork<'_> {
    /// Skips current comment evidence or runs a newly reserved collection for the discussion.
    pub async fn collect_comments(
        self,
        discussion: &Discussion,
    ) -> Result<ThreadFamilyResult<()>, EngineError> {
        let Some(collection) = CommentCollection::start(self, discussion).await? else {
            return Ok(ThreadFamilyResult::default());
        };
        collection.run().await
    }
}

/// A reserved discussion comment collection, with progress that stays provisional until finish.
struct CommentCollection<'a> {
    work: RepositoryWork<'a>,
    discussion: &'a Discussion,
    sequence: ObservationSequence,
    page_count: u32,
    result: ThreadFamilyResult<()>,
}

impl<'a> CommentCollection<'a> {
    /// Resolves already-current failures or reserves a new observation before provider paging.
    async fn start(
        work: RepositoryWork<'a>,
        discussion: &'a Discussion,
    ) -> Result<Option<Self>, EngineError> {
        let clock = SourceClock::Valid(discussion.updated_at);
        let failures = comment_failure_scope(work, discussion);
        if work
            .archive
            .child_family_is_current(
                &discussion.id,
                EvidenceFamily::Comments,
                &clock,
                comment_count(discussion),
            )
            .await?
        {
            work.archive
                .resolve_child_family_failures(work.context.lease, &failures, now_utc()?)
                .await?;
            return Ok(None);
        }
        let request_scope = format!("run:{}:{}", work.context.run_id.get(), work.unit.key);
        let reservation = work
            .archive
            .reserve_child_family_observation_fenced(
                &discussion.id,
                EvidenceFamily::Comments,
                &clock,
                now_utc()?,
                &request_scope,
                work.context.lease,
            )
            .await?;
        if !reservation.reserved {
            return Err(StoreError::StaleObservationGeneration.into());
        }
        work.archive
            .mark_child_family_failures_retried(work.context.lease, &failures)
            .await?;
        Ok(Some(Self {
            work,
            discussion,
            sequence: reservation.sequence,
            page_count: 0,
            result: ThreadFamilyResult::default(),
        }))
    }

    /// Follows REST pages until a complete collection or a provider failure ends this attempt.
    async fn run(mut self) -> Result<ThreadFamilyResult<()>, EngineError> {
        let mut next_page = None;
        loop {
            let page = match self.page(next_page.as_ref()).await {
                Ok(page) => page,
                Err(error) => return self.fail(error).await,
            };
            next_page = page.next_page;
            self.stage(page.comments).await?;
            if next_page.is_none() {
                return self.complete().await;
            }
        }
    }

    /// Requests the next page for this discussion with the run's cancellation scope.
    async fn page(&self, next: Option<&Url>) -> Result<RestCommentPage, GitHubError> {
        fetch_issue_comment_page(
            self.work.client,
            self.work.repository,
            &self.discussion.id,
            next,
            self.work.context.cancellation,
        )
        .await
    }

    /// Counts and stages members without asserting complete canonical membership.
    async fn stage(&mut self, comments: Vec<Comment>) -> Result<(), EngineError> {
        let count = u64::try_from(comments.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
        self.result.items_received = self
            .result
            .items_received
            .checked_add(count)
            .ok_or(StoreError::IntegerOutOfRange)?;
        let items = comments
            .into_iter()
            .map(|comment| StagedItem {
                id: comment.id.provider_id().clone(),
                payload: comment,
            })
            .collect::<Vec<_>>();
        self.work
            .archive
            .stage_child_family_page_fenced(
                &self.discussion.id,
                EvidenceFamily::Comments,
                self.sequence,
                self.page_count,
                &items,
                self.work.context.lease,
            )
            .await?;
        self.page_count = self
            .page_count
            .checked_add(1)
            .ok_or(StoreError::IntegerOutOfRange)?;
        Ok(())
    }

    /// Records incomplete coverage and a scoped provider failure, retaining completed-page counts.
    async fn fail(mut self, error: GitHubError) -> Result<ThreadFamilyResult<()>, EngineError> {
        self.result.pages_completed = u64::from(self.page_count);
        let completeness = CollectionCompleteness::Incomplete {
            reason: incomplete_reason(&error, self.page_count),
            received_items: self.result.items_received,
        };
        self.finish(&completeness, None).await?;
        if matches!(error, GitHubError::Cancelled) {
            self.result.interrupted = true;
        } else {
            self.record_failure(&error).await?;
        }
        Ok(self.result)
    }

    /// Writes the original provider diagnostic, retaining it if failure-ledger persistence fails.
    async fn record_failure(&mut self, error: &GitHubError) -> Result<(), EngineError> {
        let failure = github_failure(error);
        let input = RunFailureInput {
            run_id: self.work.context.run_id,
            target: &self.work.repository.full_name,
            repository: Some(&self.work.repository.id),
            thread: Some(&self.discussion.id),
            family: Some(EvidenceFamily::Comments),
            scope_key: self.work.unit.key,
            failure: &failure,
            created_at: now_utc()?,
        };
        self.work
            .archive
            .record_run_failure(self.work.context.lease, input)
            .await
            .map_err(|source| EngineError::FailureLedger {
                original: failure.clone(),
                source,
            })?;
        self.result.failure = Some(failure);
        Ok(())
    }

    /// Promotes complete membership and resolves failures only when the observation was accepted.
    async fn complete(mut self) -> Result<ThreadFamilyResult<()>, EngineError> {
        let observation = self
            .finish(&CollectionCompleteness::Complete, Some(self.page_count))
            .await?;
        self.result.pages_completed = u64::from(self.page_count);
        if matches!(
            observation.disposition,
            ObservationDisposition::Applied | ObservationDisposition::Replayed
        ) {
            self.result.items_committed = observation.item_count;
            let failures = comment_failure_scope(self.work, self.discussion);
            self.work
                .archive
                .resolve_child_family_failures(self.work.context.lease, &failures, now_utc()?)
                .await?;
        }
        Ok(self.result)
    }

    /// Applies one terminal coverage state to the reserved observation under the writer lease.
    async fn finish(
        &self,
        completeness: &CollectionCompleteness,
        expected_pages: Option<u32>,
    ) -> Result<forgesync_store::observations::FamilyObservationResult, EngineError> {
        let observation = ChildFamilyObservation {
            thread: &self.discussion.id,
            family: EvidenceFamily::Comments,
            sequence: self.sequence,
            observed_at: now_utc()?,
            completeness,
            expected_pages,
            head_sha: None,
        };
        self.work
            .archive
            .finish_child_family_observation_fenced(observation, self.work.context.lease)
            .await
            .map_err(Into::into)
    }
}

/// Names this discussion's independent comment failures within the selected repository scope.
fn comment_failure_scope<'a>(
    work: RepositoryWork<'a>,
    discussion: &'a Discussion,
) -> ChildFamilyFailureScope<'a> {
    ChildFamilyFailureScope {
        run_id: work.context.run_id,
        repository: &work.repository.id,
        thread: &discussion.id,
        family: EvidenceFamily::Comments,
        scope_key: work.unit.key,
    }
}

/// Classifies why partial provider pages cannot replace complete membership.
pub fn incomplete_reason(error: &GitHubError, pages_completed: u32) -> IncompleteReason {
    if matches!(error, GitHubError::Cancelled) {
        IncompleteReason::Cancelled
    } else if matches!(error, GitHubError::Deferred { .. }) {
        IncompleteReason::RetryBudget
    } else if pages_completed > 0 {
        IncompleteReason::Pagination
    } else {
        IncompleteReason::Unknown
    }
}

/// Reads the source count used alongside explicit coverage to decide whether acquisition is needed.
pub fn comment_count(discussion: &Discussion) -> Option<u64> {
    discussion
        .provider_data
        .get("comments")
        .and_then(serde_json::Value::as_u64)
}
