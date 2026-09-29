//! Pull requests sync work.

use super::*;

pub(super) async fn run_pull_request_jobs(
    archive: &Archive,
    client: &GitHubClient,
    repository: &forgesync_core::content::Repository,
    unit: ScopeUnit,
    context: &SyncRunContext<'_>,
    summary: &mut WorkSummary,
) -> Result<(), EngineError> {
    let targets = pull_request_targets(archive, repository, unit).await?;
    if targets.is_empty() {
        return Ok(());
    }
    let added_jobs =
        1 + u64::from(context.include_reviews) + u64::from(context.include_review_threads);
    summary.total_jobs = summary
        .total_jobs
        .checked_add(added_jobs)
        .ok_or(StoreError::IntegerOutOfRange)?;

    let started_at = now_utc()?;
    let metadata_job_id = archive
        .start_sync_job(
            context.lease,
            context.run_id,
            &repository.id,
            EvidenceFamily::PullRequestMetadata,
            unit.key,
            started_at,
        )
        .await?;
    let reviews_job_id = if context.include_reviews {
        Some(
            archive
                .start_sync_job(
                    context.lease,
                    context.run_id,
                    &repository.id,
                    EvidenceFamily::Reviews,
                    unit.key,
                    started_at,
                )
                .await?,
        )
    } else {
        None
    };
    let review_threads_job_id = if context.include_review_threads {
        Some(
            archive
                .start_sync_job(
                    context.lease,
                    context.run_id,
                    &repository.id,
                    EvidenceFamily::ReviewThreads,
                    unit.key,
                    started_at,
                )
                .await?,
        )
    } else {
        None
    };
    let progress_repository = RepositorySelector::from_repository(repository).as_url();
    send_progress(
        &context.progress,
        context.run_id,
        summary,
        summary.total_jobs,
        Some(progress_repository.clone()),
        SyncProgressStatus::InProgress,
    );

    let mut metadata_job = FamilyJobAccumulator::default();
    let mut reviews_job = FamilyJobAccumulator::default();
    let mut review_threads_job = FamilyJobAccumulator::default();
    for target in targets {
        if context.cancellation.is_cancelled() {
            metadata_job.interrupted = true;
            reviews_job.interrupted = context.include_reviews;
            review_threads_job.interrupted = context.include_review_threads;
            break;
        }
        let family_scope = ThreadFamilyScope {
            repository,
            thread: &target.thread,
            updated_at: target.updated_at,
            key: unit.key,
        };
        let metadata_result =
            sync_thread_pull_request_metadata(archive, client, &family_scope, context).await?;
        accumulate_thread_result(&mut metadata_job, &metadata_result)?;
        summary.pull_request_metadata_seen = summary
            .pull_request_metadata_seen
            .checked_add(metadata_result.items_received)
            .ok_or(StoreError::IntegerOutOfRange)?;

        if metadata_result.interrupted {
            metadata_job.interrupted = true;
            reviews_job.interrupted = context.include_reviews;
            review_threads_job.interrupted = context.include_review_threads;
            break;
        }
        if context.include_reviews {
            let reviews_result = sync_thread_reviews(
                archive,
                client,
                &family_scope,
                metadata_result.value.as_ref(),
                metadata_result.failure.as_ref(),
                context,
            )
            .await?;
            accumulate_thread_result(&mut reviews_job, &reviews_result)?;
            summary.reviews_seen = summary
                .reviews_seen
                .checked_add(reviews_result.items_received)
                .ok_or(StoreError::IntegerOutOfRange)?;
            if reviews_result.interrupted {
                metadata_job.interrupted = true;
                reviews_job.interrupted = true;
                review_threads_job.interrupted = context.include_review_threads;
                break;
            }
        }
        if context.include_review_threads {
            let review_threads_result = sync_thread_review_threads(
                archive,
                client,
                &family_scope,
                metadata_result.value.as_ref(),
                metadata_result.failure.as_ref(),
                context,
            )
            .await?;
            accumulate_thread_result(&mut review_threads_job, &review_threads_result)?;
            summary.review_threads_seen = summary
                .review_threads_seen
                .checked_add(review_threads_result.items_received)
                .ok_or(StoreError::IntegerOutOfRange)?;
            if review_threads_result.interrupted {
                metadata_job.interrupted = true;
                review_threads_job.interrupted = true;
                break;
            }
        }
    }

    summary.pages_completed = summary
        .pages_completed
        .checked_add(metadata_job.pages_completed)
        .and_then(|count| count.checked_add(reviews_job.pages_completed))
        .and_then(|count| count.checked_add(review_threads_job.pages_completed))
        .ok_or(StoreError::IntegerOutOfRange)?;
    finish_family_sync_job(
        archive,
        context,
        summary,
        &progress_repository,
        metadata_job_id,
        metadata_job,
    )
    .await?;
    if let Some(reviews_job_id) = reviews_job_id {
        finish_family_sync_job(
            archive,
            context,
            summary,
            &progress_repository,
            reviews_job_id,
            reviews_job,
        )
        .await?;
    }
    if let Some(review_threads_job_id) = review_threads_job_id {
        finish_family_sync_job(
            archive,
            context,
            summary,
            &progress_repository,
            review_threads_job_id,
            review_threads_job,
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn pull_request_targets(
    archive: &Archive,
    repository: &forgesync_core::content::Repository,
    unit: ScopeUnit,
) -> Result<Vec<PullRequestTarget>, EngineError> {
    let page_limit = NonZeroU32::new(1000).ok_or(EngineError::InvalidPageLimit)?;
    let mut offset = 0_u64;
    let mut targets = Vec::new();
    loop {
        let page = archive
            .query_threads(&ThreadQuery {
                repositories: vec![repository.id.clone()],
                kind: Some(ThreadKind::PullRequest),
                state: store_state_filter(unit.state),
                match_expression: None,
                updated_since: None,
                sort: ThreadSort::Updated,
                limit: page_limit,
                offset,
            })
            .await?;
        targets.extend(page.items.into_iter().map(|summary| PullRequestTarget {
            thread: summary.discussion.id,
            updated_at: summary.discussion.updated_at,
        }));
        let Some(next_offset) = page.next_offset else {
            break;
        };
        offset = next_offset;
    }
    Ok(targets)
}

pub(super) fn accumulate_thread_result<T>(
    job: &mut FamilyJobAccumulator,
    result: &ThreadFamilyResult<T>,
) -> Result<(), StoreError> {
    job.pages_completed = job
        .pages_completed
        .checked_add(result.pages_completed)
        .ok_or(StoreError::IntegerOutOfRange)?;
    job.items_committed = job
        .items_committed
        .checked_add(result.items_committed)
        .ok_or(StoreError::IntegerOutOfRange)?;
    job.interrupted |= result.interrupted;
    if let Some(failure) = result.failure.as_ref() {
        let failure_slot = if failure.kind == FailureKind::RateLimited {
            &mut job.deferred_failure
        } else {
            &mut job.hard_failure
        };
        if failure_slot.is_none() {
            *failure_slot = Some(failure.clone());
        }
    }
    Ok(())
}

pub(super) async fn finish_family_sync_job(
    archive: &Archive,
    context: &SyncRunContext<'_>,
    summary: &mut WorkSummary,
    repository: &str,
    job_id: i64,
    job: FamilyJobAccumulator,
) -> Result<(), EngineError> {
    let (status, failure, progress_status) = if job.interrupted {
        (
            SyncJobStatus::Interrupted,
            None,
            SyncProgressStatus::Interrupted,
        )
    } else if let Some(failure) = job.hard_failure {
        (
            SyncJobStatus::Failed,
            Some(failure.clone()),
            progress_status(&failure),
        )
    } else if let Some(failure) = job.deferred_failure {
        (
            SyncJobStatus::Deferred,
            Some(failure.clone()),
            progress_status(&failure),
        )
    } else {
        (SyncJobStatus::Complete, None, SyncProgressStatus::Complete)
    };
    archive
        .finish_sync_job(
            context.lease,
            job_id,
            SyncJobCompletion {
                status,
                updated_at: now_utc()?,
                pages_completed: job.pages_completed,
                items_committed: job.items_committed,
                failure: failure.as_ref(),
            },
        )
        .await?;
    summary.completed_jobs = summary.completed_jobs.saturating_add(1);
    if let Some(failure) = failure.as_ref() {
        count_failure(summary, failure);
    }
    if job.interrupted {
        summary.interrupted = true;
        summary.interrupted_jobs = summary.interrupted_jobs.saturating_add(1);
    }
    send_progress(
        &context.progress,
        context.run_id,
        summary,
        summary.total_jobs,
        Some(repository.to_owned()),
        progress_status,
    );
    Ok(())
}
