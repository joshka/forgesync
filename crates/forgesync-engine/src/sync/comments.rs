//! Comments sync work.

use super::support::{count_failure, progress_status, send_progress, store_state_filter};
use super::{
    Archive, ChildFamilyFailureScope, ChildFamilyObservation, CollectionCompleteness, Comment,
    CommentThreadResult, EngineError, EvidenceFamily, FailureKind, GitHubClient, GitHubError,
    IncompleteReason, NonZeroU32, ObservationDisposition, RepositorySelector, RunFailureInput,
    RunFailureScope, ScopeUnit, SourceClock, StagedItem, StoreError, SyncJobCompletion,
    SyncJobStatus, SyncProgressStatus, SyncRunContext, ThreadQuery, ThreadSort, WorkSummary,
    fetch_issue_comment_page, github_failure, now_utc,
};

/// Acquires and applies one comment-family job under the current run.
pub async fn run_comment_job(
    archive: &Archive,
    client: &GitHubClient,
    repository: &forgesync_core::content::Repository,
    unit: ScopeUnit,
    context: &SyncRunContext<'_>,
    summary: &mut WorkSummary,
) -> Result<(), EngineError> {
    let started_at = now_utc()?;
    let job_id = archive
        .start_sync_job(
            context.lease,
            context.run_id,
            &repository.id,
            EvidenceFamily::Comments,
            unit.key,
            started_at,
        )
        .await?;
    let selector_target = RepositorySelector::from_repository(repository).as_url();
    archive
        .mark_scope_failures_retried(
            context.lease,
            &RunFailureScope {
                run_id: context.run_id,
                target: &selector_target,
                family: EvidenceFamily::Comments,
                scope_key: unit.key,
            },
        )
        .await?;
    let progress_repository = RepositorySelector::from_repository(repository).as_url();
    send_progress(
        &context.progress,
        context.run_id,
        summary,
        summary.total_jobs,
        Some(progress_repository.clone()),
        SyncProgressStatus::InProgress,
    );

    let mut pages_completed = 0_u64;
    let mut comments_seen = 0_u64;
    let mut items_committed = 0_u64;
    let mut first_hard_failure = None;
    let mut first_deferred_failure = None;
    let mut interrupted = false;
    let page_limit = NonZeroU32::new(1000).ok_or(EngineError::InvalidPageLimit)?;
    let mut offset = 0_u64;

    loop {
        if context.cancellation.is_cancelled() {
            interrupted = true;
            break;
        }
        let thread_page = archive
            .query_threads(&ThreadQuery {
                repositories: vec![repository.id.clone()],
                kind: None,
                state: store_state_filter(unit.state),
                match_expression: None,
                updated_since: None,
                sort: ThreadSort::Updated,
                limit: page_limit,
                offset,
            })
            .await?;

        for thread in thread_page.items {
            if context.cancellation.is_cancelled() {
                interrupted = true;
                break;
            }
            let result = sync_thread_comments(
                archive,
                client,
                &thread.repository,
                &thread.discussion,
                unit.key,
                context,
            )
            .await?;
            pages_completed = pages_completed
                .checked_add(result.pages_completed)
                .ok_or(StoreError::IntegerOutOfRange)?;
            comments_seen = comments_seen
                .checked_add(result.comments_received)
                .ok_or(StoreError::IntegerOutOfRange)?;
            items_committed = items_committed
                .checked_add(result.comments_committed)
                .ok_or(StoreError::IntegerOutOfRange)?;
            if let Some(failure) = result.failure {
                if failure.kind == FailureKind::RateLimited {
                    if first_deferred_failure.is_none() {
                        first_deferred_failure = Some(failure);
                    }
                } else if first_hard_failure.is_none() {
                    first_hard_failure = Some(failure);
                }
            }
            if result.interrupted {
                interrupted = true;
                break;
            }
        }
        if interrupted {
            break;
        }
        let Some(next_offset) = thread_page.next_offset else {
            break;
        };
        offset = next_offset;
    }

    summary.pages_completed = summary
        .pages_completed
        .checked_add(pages_completed)
        .ok_or(StoreError::IntegerOutOfRange)?;
    summary.comments_seen = summary
        .comments_seen
        .checked_add(comments_seen)
        .ok_or(StoreError::IntegerOutOfRange)?;

    let (status, failure) = if interrupted {
        (SyncJobStatus::Interrupted, None)
    } else if let Some(failure) = first_hard_failure {
        (SyncJobStatus::Failed, Some(failure))
    } else if let Some(failure) = first_deferred_failure {
        (SyncJobStatus::Deferred, Some(failure))
    } else {
        (SyncJobStatus::Complete, None)
    };
    archive
        .finish_sync_job(
            context.lease,
            job_id,
            SyncJobCompletion {
                status,
                updated_at: now_utc()?,
                pages_completed,
                items_committed,
                // Thread-specific failures already have their own durable records.
                failure: None,
            },
        )
        .await?;
    if status == SyncJobStatus::Complete {
        archive
            .resolve_scope_failures(
                context.lease,
                &RunFailureScope {
                    run_id: context.run_id,
                    target: &selector_target,
                    family: EvidenceFamily::Comments,
                    scope_key: unit.key,
                },
                now_utc()?,
            )
            .await?;
    }
    summary.completed_jobs += 1;
    if let Some(failure) = failure.as_ref() {
        count_failure(summary, failure);
    }
    if interrupted {
        summary.interrupted = true;
        summary.interrupted_jobs += 1;
    }
    send_progress(
        &context.progress,
        context.run_id,
        summary,
        summary.total_jobs,
        Some(progress_repository),
        if interrupted {
            SyncProgressStatus::Interrupted
        } else if let Some(failure) = failure.as_ref() {
            progress_status(failure)
        } else {
            SyncProgressStatus::Complete
        },
    );
    Ok(())
}

/// Classifies why a partially paged collection cannot replace complete membership.
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

/// Stages every comment page before completing a thread's comment family.
pub async fn sync_thread_comments(
    archive: &Archive,
    client: &GitHubClient,
    repository: &forgesync_core::content::Repository,
    discussion: &forgesync_core::content::Discussion,
    scope_key: &str,
    context: &SyncRunContext<'_>,
) -> Result<CommentThreadResult, EngineError> {
    let failure_scope = ChildFamilyFailureScope {
        run_id: context.run_id,
        repository: &repository.id,
        thread: &discussion.id,
        family: EvidenceFamily::Comments,
        scope_key,
    };
    let source_clock = SourceClock::Valid(discussion.updated_at);
    let expected_item_count = comment_count(discussion);
    if archive
        .child_family_is_current(
            &discussion.id,
            EvidenceFamily::Comments,
            &source_clock,
            expected_item_count,
        )
        .await?
    {
        archive
            .resolve_child_family_failures(context.lease, &failure_scope, now_utc()?)
            .await?;
        return Ok(CommentThreadResult::default());
    }

    let started_at = now_utc()?;
    let request_scope = format!("run:{}:{}", context.run_id.get(), scope_key);
    let reservation = archive
        .reserve_child_family_observation_fenced(
            &discussion.id,
            EvidenceFamily::Comments,
            &source_clock,
            started_at,
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

    let mut result = CommentThreadResult::default();
    let mut next_page = None;
    let mut page_count = 0_u32;
    loop {
        let page = match fetch_issue_comment_page(
            client,
            repository,
            &discussion.id,
            next_page.as_ref(),
            context.cancellation,
        )
        .await
        {
            Ok(page) => page,
            Err(error) => {
                result.pages_completed = u64::from(page_count);
                let incomplete_reason = if matches!(error, GitHubError::Cancelled) {
                    forgesync_core::observation::IncompleteReason::Cancelled
                } else if matches!(error, GitHubError::Deferred { .. }) {
                    forgesync_core::observation::IncompleteReason::RetryBudget
                } else if page_count > 0 {
                    forgesync_core::observation::IncompleteReason::Pagination
                } else {
                    forgesync_core::observation::IncompleteReason::Unknown
                };
                let failure = github_failure(&error);
                archive
                    .finish_child_family_observation_fenced(
                        ChildFamilyObservation {
                            thread: &discussion.id,
                            family: EvidenceFamily::Comments,
                            sequence: reservation.sequence,
                            observed_at: now_utc()?,
                            completeness: &CollectionCompleteness::Incomplete {
                                reason: incomplete_reason,
                                received_items: result.comments_received,
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
                archive
                    .record_run_failure(
                        context.lease,
                        RunFailureInput {
                            run_id: context.run_id,
                            target: &repository.full_name,
                            repository: Some(&repository.id),
                            thread: Some(&discussion.id),
                            family: Some(EvidenceFamily::Comments),
                            scope_key,
                            failure: &failure,
                            created_at: now_utc()?,
                        },
                    )
                    .await
                    .map_err(|source| EngineError::FailureLedger {
                        original: failure.clone(),
                        source,
                    })?;
                result.failure = Some(failure);
                return Ok(result);
            }
        };

        let item_count =
            u64::try_from(page.comments.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
        result.comments_received = result
            .comments_received
            .checked_add(item_count)
            .ok_or(StoreError::IntegerOutOfRange)?;
        let items = page
            .comments
            .into_iter()
            .map(|comment| StagedItem {
                id: comment.id.provider_id().clone(),
                payload: comment,
            })
            .collect::<Vec<StagedItem<Comment>>>();
        archive
            .stage_child_family_page_fenced(
                &discussion.id,
                EvidenceFamily::Comments,
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
                thread: &discussion.id,
                family: EvidenceFamily::Comments,
                sequence: reservation.sequence,
                observed_at: now_utc()?,
                completeness: &CollectionCompleteness::Complete,
                expected_pages: Some(page_count),
                head_sha: None,
            },
            context.lease,
        )
        .await?;
    result.pages_completed = u64::from(page_count);
    if matches!(
        observation.disposition,
        ObservationDisposition::Applied | ObservationDisposition::Replayed
    ) {
        result.comments_committed = observation.item_count;
        archive
            .resolve_child_family_failures(context.lease, &failure_scope, now_utc()?)
            .await?;
    }
    Ok(result)
}

/// Reads the source comment count used to decide whether acquisition is needed.
pub fn comment_count(discussion: &forgesync_core::content::Discussion) -> Option<u64> {
    discussion
        .provider_data
        .get("comments")
        .and_then(serde_json::Value::as_u64)
}
