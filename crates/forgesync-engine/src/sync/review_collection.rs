//! # Durable lifecycle of one pull-request review family
//!
//! Reviews and review threads use different provider pagination, but share the same archive
//! protocol. `ReviewSync` identifies the family and its run scope before acquisition begins.
//! Preparing it either returns a finished result (already current or missing head metadata), or
//! a `ReviewCollection` with a reserved observation and a known pull-request head.
//!
//! The collection owns page counts and staged-item counts until a consuming terminal operation
//! records complete or incomplete coverage. Provider collectors keep their own page links and
//! cursor checks; they never manipulate these counters or repeat archive finalization policy.
//! Only an applied or replayed complete observation resolves old failures. Cancellation records
//! incomplete coverage without creating a new provider-failure entry.
//!
//! `ReviewFamily` couples the selected traversal to the archive's evidence-family identity. The
//! caller creates a `ReviewSync` with explicit services, a parent scope, and run ownership, then
//! calls `run` with the metadata result. Keeping its value and failure together prevents callers
//! from supplying a head from one result and a diagnostic from another.
//!
//! Preparation checks the source clock and head before reserving. Current evidence resolves old
//! failures and returns an empty work result. Missing metadata still gets a reserved incomplete
//! observation and a scoped failure, making the skipped acquisition inspectable in the archive.
//! A ready collection carries its known head, sequence, and provisional counts through pagination.
//!
//! Staging increments received counts and persists pages without changing canonical membership.
//! Complete finalization reports committed members and clears satisfied failures. Provider failure
//! records incomplete coverage and preserves the provider diagnostic; cancellation marks the result
//! interrupted. Store errors propagate as engine errors, since an unsuccessful archive write cannot
//! be represented as a successfully recorded provider failure.

use forgesync_core::content::PullRequestMetadata;
use forgesync_core::coverage::{EvidenceFamily, Failure, FailureKind};
use forgesync_core::identity::{CommitSha, ObservationSequence};
use forgesync_core::observation::{CollectionCompleteness, IncompleteReason, SourceClock};
use forgesync_github::error::GitHubError;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::families::{ChildFamilyObservation, ChildFamilyPage, ChildFamilyRequest};
use forgesync_store::observations::{ObservationDisposition, StagedItem};
use forgesync_store::runs::ChildFamilyFailureScope;
use serde::Serialize;

use super::comments::incomplete_reason;
use super::{SyncRunContext, ThreadFamilyResult, ThreadFamilyScope};
use crate::clock::now_utc;
use crate::error::EngineError;
use crate::provider_failure::github_failure;

/// A review family's durable write scope before it has an observation reservation.
pub struct ReviewSync<'a> {
    /// Archive owning reservations, staged pages, canonical membership, and failure recovery.
    archive: &'a Archive,
    /// Provider client chosen by the caller for this repository host.
    pub client: &'a GitHubClient,
    /// Parent identity, source update time, and durable scope key for this acquisition.
    pub scope: ThreadFamilyScope<'a>,
    /// Run ownership, cancellation, and lease shared with sibling family work.
    pub context: &'a SyncRunContext<'a>,
    /// Couples provider traversal to the evidence family used in all durable writes.
    family: ReviewFamily,
}

/// Selects a provider traversal and its matching archive evidence family together.
#[derive(Clone, Copy)]
pub enum ReviewFamily {
    /// REST review records attached to the pull request.
    Reviews,
    /// GraphQL review threads and their nested comments.
    ReviewThreads,
}

/// Preparation either needs provider work or has already produced the family's terminal result.
pub enum ReviewPreparation<'a> {
    /// A reserved collection whose head is available for provider acquisition.
    Ready(ReviewCollection<'a>),
    /// Fresh evidence or unavailable metadata requires no provider-page request.
    Finished(ThreadFamilyResult<()>),
}

/// A reserved review observation and its provisional page progress.
///
/// The known head belongs to this acquisition. Consuming completion methods prevent a collector
/// from treating a finished attempt as one that can accept another page.
pub struct ReviewCollection<'a> {
    /// Services and parent scope retained for every staged and terminal write.
    pub target: ReviewSync<'a>,
    /// Pull-request head captured from the metadata result before page acquisition.
    pub head: CommitSha,
    /// Reserved generation checked by staging and consuming finalization.
    sequence: ObservationSequence,
    /// Successfully staged pages; also the next zero-based page index and terminal page count.
    page_count: u32,
    /// Provisional received-item accounting; completion fills committed counts from the store.
    /// Received items can include repeated identities and are not canonical membership size.
    result: ThreadFamilyResult<()>,
}

impl<'a> ReviewSync<'a> {
    /// Binds the archive, thread scope, and run ownership used by every write in this attempt.
    pub fn new(
        archive: &'a Archive,
        client: &'a GitHubClient,
        scope: ThreadFamilyScope<'a>,
        context: &'a SyncRunContext<'a>,
        family: ReviewFamily,
    ) -> Self {
        Self {
            archive,
            client,
            scope,
            context,
            family,
        }
    }

    /// Runs the provider traversal selected by the family, retaining scoped partial outcomes.
    pub async fn run(
        self,
        metadata: &ThreadFamilyResult<PullRequestMetadata>,
    ) -> Result<ThreadFamilyResult<()>, EngineError> {
        match self.family {
            ReviewFamily::Reviews => self.sync_reviews(metadata).await,
            ReviewFamily::ReviewThreads => self.sync_review_threads(metadata).await,
        }
    }

    /// Skips current evidence or reserves a new attempt before any provider-page request.
    pub async fn prepare(
        self,
        metadata: &ThreadFamilyResult<PullRequestMetadata>,
    ) -> Result<ReviewPreparation<'a>, EngineError> {
        if let Some(value) = &metadata.value
            && self.is_current(&value.head.sha).await?
        {
            self.resolve_failures().await?;
            return Ok(ReviewPreparation::Finished(ThreadFamilyResult::default()));
        }
        let sequence = self.reserve().await?;
        match &metadata.value {
            Some(value) => Ok(ReviewPreparation::Ready(ReviewCollection {
                target: self,
                head: value.head.sha.clone(),
                sequence,
                page_count: 0,
                result: ThreadFamilyResult::default(),
            })),
            None => self.missing_head(sequence, metadata.failure.as_ref()).await,
        }
    }

    /// Checks both source clock and head, since matching parent timestamps alone are insufficient.
    async fn is_current(&self, head: &CommitSha) -> Result<bool, EngineError> {
        self.archive
            .pull_request_family_is_current_for_head(
                self.scope.thread,
                self.family.evidence_family(),
                &SourceClock::Valid(self.scope.updated_at),
                head,
            )
            .await
            .map_err(Into::into)
    }

    /// Reserves local order and marks earlier family failures as having a new retry attempt.
    async fn reserve(&self) -> Result<ObservationSequence, EngineError> {
        let request_scope = format!("run:{}:{}", self.context.run_id.get(), self.scope.key);
        let reservation = self
            .archive
            .reserve_child_family_observation_fenced(
                ChildFamilyRequest {
                    thread: self.scope.thread,
                    family: self.family.evidence_family(),
                    source_clock: &SourceClock::Valid(self.scope.updated_at),
                    started_at: now_utc()?,
                    request_scope: &request_scope,
                },
                self.context.lease,
            )
            .await?;
        if !reservation.reserved {
            return Err(StoreError::StaleObservationGeneration.into());
        }
        self.archive
            .mark_child_family_failures_retried(self.context.lease, &self.failure_scope())
            .await?;
        Ok(reservation.sequence)
    }

    /// Records missing metadata as incomplete coverage instead of acquiring unscoped reviews.
    async fn missing_head(
        self,
        sequence: ObservationSequence,
        metadata_failure: Option<&Failure>,
    ) -> Result<ReviewPreparation<'a>, EngineError> {
        let failure = metadata_failure.cloned().unwrap_or(Failure {
            kind: FailureKind::ProviderResponse,
            message: "pull-request head metadata is unavailable".to_owned(),
        });
        self.finish_incomplete(sequence, 0, IncompleteReason::Unknown)
            .await?;
        self.record_failure(&failure).await?;
        let result = ThreadFamilyResult {
            failure: Some(failure),
            ..Default::default()
        };
        Ok(ReviewPreparation::Finished(result))
    }

    /// Finalizes a partial attempt without changing previously complete canonical membership.
    async fn finish_incomplete(
        &self,
        sequence: ObservationSequence,
        received_items: u64,
        reason: IncompleteReason,
    ) -> Result<(), EngineError> {
        let completeness = CollectionCompleteness::Incomplete {
            reason,
            received_items,
        };
        let observation = ChildFamilyObservation {
            thread: self.scope.thread,
            family: self.family.evidence_family(),
            sequence,
            observed_at: now_utc()?,
            completeness: &completeness,
            expected_pages: None,
            head_sha: None,
        };
        self.archive
            .finish_child_family_observation_fenced(observation, self.context.lease)
            .await?;
        Ok(())
    }

    /// Attributes a provider failure to this run's thread and evidence family.
    async fn record_failure(&self, failure: &Failure) -> Result<(), EngineError> {
        self.scope
            .record_failure(
                self.archive,
                self.context,
                self.family.evidence_family(),
                failure,
            )
            .await
    }

    /// Clears earlier failures only when current or newly completed evidence satisfies the family.
    async fn resolve_failures(&self) -> Result<(), EngineError> {
        self.archive
            .resolve_child_family_failures(self.context.lease, &self.failure_scope(), now_utc()?)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// Names the durable failure scope shared by reservation, resolution, and reporting.
    fn failure_scope(&self) -> ChildFamilyFailureScope<'_> {
        ChildFamilyFailureScope {
            run_id: self.context.run_id,
            repository: &self.scope.repository.id,
            thread: self.scope.thread,
            family: self.family.evidence_family(),
            scope_key: self.scope.key,
        }
    }
}

impl ReviewCollection<'_> {
    /// Counts received items and stages a page; membership stays provisional until completion.
    pub async fn stage<T: Serialize>(
        &mut self,
        items: &[StagedItem<T>],
    ) -> Result<(), EngineError> {
        let count = u64::try_from(items.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
        self.result.items_received = self
            .result
            .items_received
            .checked_add(count)
            .ok_or(StoreError::IntegerOutOfRange)?;
        self.target
            .archive
            .stage_child_family_page_fenced(
                ChildFamilyPage {
                    thread: self.target.scope.thread,
                    family: self.target.family.evidence_family(),
                    sequence: self.sequence,
                    page_index: self.page_count,
                    items,
                },
                self.target.context.lease,
            )
            .await?;
        self.page_count = self
            .page_count
            .checked_add(1)
            .ok_or(StoreError::IntegerOutOfRange)?;
        Ok(())
    }

    /// Ends provider acquisition with incomplete coverage and the appropriate failure outcome.
    pub async fn fail(mut self, error: GitHubError) -> Result<ThreadFamilyResult<()>, EngineError> {
        let reason = incomplete_reason(&error, self.page_count);
        self.target
            .finish_incomplete(self.sequence, self.result.items_received, reason)
            .await?;
        if matches!(error, GitHubError::Cancelled) {
            self.result.interrupted = true;
        } else {
            let failure = github_failure(&error);
            self.target.record_failure(&failure).await?;
            self.result.failure = Some(failure);
            self.result.pages_completed = u64::from(self.page_count);
        }
        Ok(self.result)
    }

    /// Commits a validated complete family for this head and resolves its earlier failures.
    pub async fn complete(mut self) -> Result<ThreadFamilyResult<()>, EngineError> {
        let observation = ChildFamilyObservation {
            thread: self.target.scope.thread,
            family: self.target.family.evidence_family(),
            sequence: self.sequence,
            observed_at: now_utc()?,
            completeness: &CollectionCompleteness::Complete,
            expected_pages: Some(self.page_count),
            head_sha: Some(&self.head),
        };
        let result = self
            .target
            .archive
            .finish_child_family_observation_fenced(observation, self.target.context.lease)
            .await?;
        if !matches!(
            result.disposition,
            ObservationDisposition::Applied | ObservationDisposition::Replayed
        ) {
            return Err(StoreError::StaleObservationGeneration.into());
        }
        self.result.pages_completed = u64::from(self.page_count);
        self.result.items_committed = result.item_count;
        self.target.resolve_failures().await?;
        Ok(self.result)
    }
}

impl ReviewFamily {
    /// Maps the chosen traversal to the store's independently covered evidence family.
    fn evidence_family(self) -> EvidenceFamily {
        match self {
            Self::Reviews => EvidenceFamily::Reviews,
            Self::ReviewThreads => EvidenceFamily::ReviewThreads,
        }
    }
}
