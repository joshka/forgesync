//! # Acquire pull-request metadata for a thread
//!
//! Metadata synchronization obtains pull-request-specific fields and applies their normalized
//! observation to the archive. It is invoked only for a discussion whose kind and requested
//! evidence make that work relevant.
//!
//! The parent discussion and child review families have separate ownership. Keeping this step
//! distinct prevents a metadata fetch from accidentally asserting that reviews or review threads
//! were collected.

use super::comments::incomplete_reason;
use super::support::record_thread_family_failure;
use super::{
    Archive, ChildFamilyFailureScope, ChildFamilyObservation, CollectionCompleteness, EngineError,
    EvidenceFamily, GitHubClient, GitHubError, ObservationDisposition, PullRequestMetadata,
    SourceClock, StagedItem, StoreError, SyncRunContext, ThreadFamilyResult, ThreadFamilyScope,
    fetch_pull_request_metadata, github_failure, now_utc,
};

/// Acquires head-bound pull-request metadata as an independent evidence family.
pub async fn sync_thread_pull_request_metadata(
    archive: &Archive,
    client: &GitHubClient,
    scope: &ThreadFamilyScope<'_>,
    context: &SyncRunContext<'_>,
) -> Result<ThreadFamilyResult<PullRequestMetadata>, EngineError> {
    let mut result = ThreadFamilyResult::default();
    let family = EvidenceFamily::PullRequestMetadata;
    let failure_scope = ChildFamilyFailureScope {
        run_id: context.run_id,
        repository: &scope.repository.id,
        thread: scope.thread,
        family,
        scope_key: scope.key,
    };
    let source_clock = SourceClock::Valid(scope.updated_at);
    let request_scope = format!("run:{}:{}", context.run_id.get(), scope.key);
    let reservation = archive
        .reserve_child_family_observation_fenced(
            scope.thread,
            family,
            &source_clock,
            now_utc()?,
            &request_scope,
            context.lease,
        )
        .await?;
    if !reservation.reserved {
        return Err(StoreError::StaleObservationGeneration.into());
    }
    archive
        .mark_child_family_failures_retried(context.lease, &failure_scope)
        .await?;

    let metadata = match fetch_pull_request_metadata(
        client,
        scope.repository,
        scope.thread,
        context.cancellation,
    )
    .await
    {
        Ok(metadata) => metadata,
        Err(error) => {
            archive
                .finish_child_family_observation_fenced(
                    ChildFamilyObservation {
                        thread: scope.thread,
                        family,
                        sequence: reservation.sequence,
                        observed_at: now_utc()?,
                        completeness: &CollectionCompleteness::Incomplete {
                            reason: incomplete_reason(&error, 0),
                            received_items: 0,
                        },
                        expected_pages: None,
                        head_sha: None,
                    },
                    context.lease,
                )
                .await?;
            if matches!(error, GitHubError::Cancelled) {
                result.interrupted = true;
                return Ok(result);
            }
            let failure = github_failure(&error);
            record_thread_family_failure(
                archive,
                context,
                scope.repository,
                scope.thread,
                family,
                scope.key,
                &failure,
            )
            .await?;
            result.failure = Some(failure);
            return Ok(result);
        }
    };

    let item = StagedItem {
        id: scope.thread.provider_id().clone(),
        payload: metadata.clone(),
    };
    archive
        .stage_child_family_page_fenced(
            scope.thread,
            family,
            reservation.sequence,
            0,
            &[item],
            context.lease,
        )
        .await?;
    let observation = archive
        .finish_child_family_observation_fenced(
            ChildFamilyObservation {
                thread: scope.thread,
                family,
                sequence: reservation.sequence,
                observed_at: now_utc()?,
                completeness: &CollectionCompleteness::Complete,
                expected_pages: Some(1),
                head_sha: None,
            },
            context.lease,
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
        archive
            .resolve_child_family_failures(context.lease, &failure_scope, now_utc()?)
            .await?;
    } else {
        return Err(StoreError::StaleObservationGeneration.into());
    }
    Ok(result)
}
