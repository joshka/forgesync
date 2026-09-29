//! # Reserved acquisition of pull-request head metadata
//!
//! `ThreadFamilyScope::sync_metadata` reserves an independent metadata observation before
//! contacting the provider. `MetadataObservation` keeps that reservation tied to its parent and
//! run ownership until provider failure or complete application ends the attempt.
//!
//! A failed request records incomplete coverage and a scoped diagnostic; cancellation interrupts
//! without adding a provider failure. A complete response stages one metadata member and must be
//! applied or replayed before its head is returned to review collectors. This is the dependency
//! boundary that keeps reviews attached to a known pull-request head.

use forgesync_core::content::PullRequestMetadata;
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::ObservationSequence;
use forgesync_core::observation::{CollectionCompleteness, SourceClock};
use forgesync_github::error::GitHubError;
use forgesync_github::resources::fetch_pull_request_metadata;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::families::ChildFamilyObservation;
use forgesync_store::observations::{ObservationDisposition, StagedItem};
use forgesync_store::runs::ChildFamilyFailureScope;

use super::comments::incomplete_reason;
use super::support::record_thread_family_failure;
use super::{SyncRunContext, ThreadFamilyResult, ThreadFamilyScope};
use crate::enumeration::{github_failure, now_utc};
use crate::error::EngineError;

impl ThreadFamilyScope<'_> {
    /// Acquires metadata independently and returns a head only after canonical application.
    pub async fn sync_metadata(
        &self,
        archive: &Archive,
        client: &GitHubClient,
        context: &SyncRunContext<'_>,
    ) -> Result<ThreadFamilyResult<PullRequestMetadata>, EngineError> {
        let observation = MetadataObservation::reserve(archive, client, *self, context).await?;
        observation.acquire().await
    }
}

/// Reserved metadata work whose provider result is not yet canonical.
struct MetadataObservation<'a> {
    archive: &'a Archive,
    client: &'a GitHubClient,
    scope: ThreadFamilyScope<'a>,
    context: &'a SyncRunContext<'a>,
    sequence: ObservationSequence,
}

impl<'a> MetadataObservation<'a> {
    /// Reserves acquisition order and marks prior metadata failures as retried.
    async fn reserve(
        archive: &'a Archive,
        client: &'a GitHubClient,
        scope: ThreadFamilyScope<'a>,
        context: &'a SyncRunContext<'a>,
    ) -> Result<Self, EngineError> {
        let source_clock = SourceClock::Valid(scope.updated_at);
        let request_scope = format!("run:{}:{}", context.run_id.get(), scope.key);
        let reservation = archive
            .reserve_child_family_observation_fenced(
                scope.thread,
                EvidenceFamily::PullRequestMetadata,
                &source_clock,
                now_utc()?,
                &request_scope,
                context.lease,
            )
            .await?;
        if !reservation.reserved {
            return Err(StoreError::StaleObservationGeneration.into());
        }
        let metadata = Self {
            archive,
            client,
            scope,
            context,
            sequence: reservation.sequence,
        };
        archive
            .mark_child_family_failures_retried(context.lease, &metadata.failure_scope())
            .await?;
        Ok(metadata)
    }

    /// Routes the single provider response to complete or incomplete terminal application.
    async fn acquire(self) -> Result<ThreadFamilyResult<PullRequestMetadata>, EngineError> {
        match fetch_pull_request_metadata(
            self.client,
            self.scope.repository,
            self.scope.thread,
            self.context.cancellation,
        )
        .await
        {
            Ok(metadata) => self.complete(metadata).await,
            Err(error) => self.fail(error).await,
        }
    }
    /// Records the failed provider attempt while preserving its diagnostic if ledger writes fail.
    async fn fail(
        self,
        error: GitHubError,
    ) -> Result<ThreadFamilyResult<PullRequestMetadata>, EngineError> {
        let mut result = ThreadFamilyResult::default();
        self.archive
            .finish_child_family_observation_fenced(
                ChildFamilyObservation {
                    thread: self.scope.thread,
                    family: EvidenceFamily::PullRequestMetadata,
                    sequence: self.sequence,
                    observed_at: now_utc()?,
                    completeness: &CollectionCompleteness::Incomplete {
                        reason: incomplete_reason(&error, 0),
                        received_items: 0,
                    },
                    expected_pages: None,
                    head_sha: None,
                },
                self.context.lease,
            )
            .await?;
        if matches!(error, GitHubError::Cancelled) {
            result.interrupted = true;
            return Ok(result);
        }
        let failure = github_failure(&error);
        record_thread_family_failure(
            self.archive,
            self.context,
            self.scope.repository,
            self.scope.thread,
            EvidenceFamily::PullRequestMetadata,
            self.scope.key,
            &failure,
        )
        .await?;
        result.failure = Some(failure);
        Ok(result)
    }

    /// Stages and applies the single metadata member, then exposes its head to dependent work.
    async fn complete(
        self,
        metadata: PullRequestMetadata,
    ) -> Result<ThreadFamilyResult<PullRequestMetadata>, EngineError> {
        let mut result = ThreadFamilyResult::default();
        let item = StagedItem {
            id: self.scope.thread.provider_id().clone(),
            payload: metadata.clone(),
        };
        self.archive
            .stage_child_family_page_fenced(
                self.scope.thread,
                EvidenceFamily::PullRequestMetadata,
                self.sequence,
                0,
                &[item],
                self.context.lease,
            )
            .await?;
        let observation = self
            .archive
            .finish_child_family_observation_fenced(
                ChildFamilyObservation {
                    thread: self.scope.thread,
                    family: EvidenceFamily::PullRequestMetadata,
                    sequence: self.sequence,
                    observed_at: now_utc()?,
                    completeness: &CollectionCompleteness::Complete,
                    expected_pages: Some(1),
                    head_sha: None,
                },
                self.context.lease,
            )
            .await?;
        if matches!(
            observation.disposition,
            ObservationDisposition::Applied | ObservationDisposition::Replayed
        ) {
            result.pages_completed = 1;
            result.items_received = 1;
            result.items_committed = observation.item_count;
            result.value = Some(metadata);
            self.archive
                .resolve_child_family_failures(
                    self.context.lease,
                    &self.failure_scope(),
                    now_utc()?,
                )
                .await?;
        } else {
            return Err(StoreError::StaleObservationGeneration.into());
        }
        Ok(result)
    }

    /// Identifies metadata failure records for retry marking and successful resolution.
    fn failure_scope(&self) -> ChildFamilyFailureScope<'_> {
        ChildFamilyFailureScope {
            run_id: self.context.run_id,
            repository: &self.scope.repository.id,
            thread: self.scope.thread,
            family: EvidenceFamily::PullRequestMetadata,
            scope_key: self.scope.key,
        }
    }
}
