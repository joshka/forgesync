//! Reviews sync work.

use super::comments::incomplete_reason;
use super::support::record_thread_family_failure;
use super::{
    Archive, ChildFamilyFailureScope, ChildFamilyObservation, CollectionCompleteness, EngineError,
    EvidenceFamily, Failure, FailureKind, GitHubClient, GitHubError, IncompleteReason,
    ObservationDisposition, PullRequestMetadata, Review, SourceClock, StagedItem, StoreError,
    SyncRunContext, ThreadFamilyResult, ThreadFamilyScope, fetch_pull_request_review_page,
    github_failure, now_utc,
};

pub(super) async fn sync_thread_reviews(
    archive: &Archive,
    client: &GitHubClient,
    scope: &ThreadFamilyScope<'_>,
    metadata: Option<&PullRequestMetadata>,
    metadata_failure: Option<&Failure>,
    context: &SyncRunContext<'_>,
) -> Result<ThreadFamilyResult<()>, EngineError> {
    let mut result = ThreadFamilyResult::default();
    let family = EvidenceFamily::Reviews;
    let failure_scope = ChildFamilyFailureScope {
        run_id: context.run_id,
        repository: &scope.repository.id,
        thread: scope.thread,
        family,
        scope_key: scope.key,
    };
    let source_clock = SourceClock::Valid(scope.updated_at);
    if let Some(metadata) = metadata
        && archive
            .pull_request_family_is_current_for_head(
                scope.thread,
                family,
                &source_clock,
                &metadata.head.sha,
            )
            .await?
    {
        archive
            .resolve_child_family_failures(context.lease, &failure_scope, now_utc()?)
            .await?;
        return Ok(result);
    }

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

    let Some(metadata) = metadata else {
        let failure = metadata_failure.cloned().unwrap_or(Failure {
            kind: FailureKind::ProviderResponse,
            message: "pull-request head metadata is unavailable".to_owned(),
        });
        archive
            .finish_child_family_observation_fenced(
                ChildFamilyObservation {
                    thread: scope.thread,
                    family,
                    sequence: reservation.sequence,
                    observed_at: now_utc()?,
                    completeness: &CollectionCompleteness::Incomplete {
                        reason: IncompleteReason::Unknown,
                        received_items: 0,
                    },
                    expected_pages: None,
                    head_sha: None,
                },
                context.lease,
            )
            .await?;
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
    };

    let mut next_page = None;
    let mut page_count = 0_u32;
    loop {
        let page = match fetch_pull_request_review_page(
            client,
            scope.repository,
            scope.thread,
            next_page.as_ref(),
            context.cancellation,
        )
        .await
        {
            Ok(page) => page,
            Err(error) => {
                archive
                    .finish_child_family_observation_fenced(
                        ChildFamilyObservation {
                            thread: scope.thread,
                            family,
                            sequence: reservation.sequence,
                            observed_at: now_utc()?,
                            completeness: &CollectionCompleteness::Incomplete {
                                reason: incomplete_reason(&error, page_count),
                                received_items: result.items_received,
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
                result.pages_completed = u64::from(page_count);
                return Ok(result);
            }
        };
        let page_items =
            u64::try_from(page.reviews.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
        result.items_received = result
            .items_received
            .checked_add(page_items)
            .ok_or(StoreError::IntegerOutOfRange)?;
        let items = page
            .reviews
            .into_iter()
            .map(|review| StagedItem {
                id: review.id.provider_id().clone(),
                payload: review,
            })
            .collect::<Vec<StagedItem<Review>>>();
        archive
            .stage_child_family_page_fenced(
                scope.thread,
                family,
                reservation.sequence,
                page_count,
                &items,
                context.lease,
            )
            .await?;
        page_count = page_count
            .checked_add(1)
            .ok_or(StoreError::IntegerOutOfRange)?;
        next_page = page.next_page;
        if next_page.is_none() {
            break;
        }
    }

    let observation = archive
        .finish_child_family_observation_fenced(
            ChildFamilyObservation {
                thread: scope.thread,
                family,
                sequence: reservation.sequence,
                observed_at: now_utc()?,
                completeness: &CollectionCompleteness::Complete,
                expected_pages: Some(page_count),
                head_sha: Some(&metadata.head.sha),
            },
            context.lease,
        )
        .await?;
    result.pages_completed = u64::from(page_count);
    if matches!(
        observation.disposition,
        ObservationDisposition::Applied | ObservationDisposition::Replayed
    ) {
        result.items_committed = observation.item_count;
        archive
            .resolve_child_family_failures(context.lease, &failure_scope, now_utc()?)
            .await?;
    } else {
        return Err(StoreError::StaleObservationGeneration.into());
    }
    Ok(result)
}
