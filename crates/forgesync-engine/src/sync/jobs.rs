//! Jobs sync work.

use super::comments::run_comment_job;
use super::pull_requests::run_pull_request_jobs;
use super::support::{count_failure, job_result, overlap_start, progress_status, send_progress};
use super::{
    Archive, EngineError, EvidenceFamily, GitHubClient, GitHubError, GitHubHost, HashMap,
    RepositorySelector, RepositoryThreadScanStatus, RunFailureInput, RunFailureScope, ScopeUnit,
    StoreError, SyncJobCompletion, SyncProgressStatus, SyncRunContext, ThreadListState,
    ThreadScanContext, WorkSummary, enumerate_repository_thread_pages, fetch_repository,
    github_failure, now_utc,
};

pub(super) async fn run_jobs(
    archive: &Archive,
    clients: &HashMap<GitHubHost, GitHubClient>,
    selectors: &[RepositorySelector],
    units: &[ScopeUnit],
    context: &SyncRunContext<'_>,
) -> Result<WorkSummary, EngineError> {
    let mut summary = WorkSummary {
        total_jobs: context.total_jobs,
        completed_jobs: 0,
        failed_jobs: 0,
        deferred_jobs: 0,
        pages_completed: 0,
        threads_seen: 0,
        comments_seen: 0,
        pull_request_metadata_seen: 0,
        reviews_seen: 0,
        review_threads_seen: 0,
        interrupted: false,
        interrupted_jobs: 0,
        pending_jobs: 0,
        first_failure: None,
    };

    for selector in selectors {
        if context.cancellation.is_cancelled() {
            summary.interrupted = true;
            break;
        }
        let client =
            clients
                .get(selector.host())
                .ok_or_else(|| EngineError::GitHubClientMissing {
                    host: selector.host().as_str().to_owned(),
                })?;
        let repository_started_at = now_utc()?;
        let repository_sequence = archive
            .reserve_observation_sequence_fenced(repository_started_at, context.lease)
            .await?;
        let repository = match fetch_repository(
            client,
            selector.host(),
            selector.owner(),
            selector.name(),
            context.cancellation,
        )
        .await
        {
            Ok(repository) => repository,
            Err(GitHubError::Cancelled) => {
                summary.interrupted = true;
                break;
            }
            Err(error) => {
                let failure = github_failure(&error);
                for unit in units {
                    let selected_families = [
                        Some(EvidenceFamily::Threads),
                        context.include_comments.then_some(EvidenceFamily::Comments),
                    ]
                    .into_iter()
                    .flatten();
                    for family in selected_families {
                        archive
                            .record_run_failure(
                                context.lease,
                                RunFailureInput {
                                    run_id: context.run_id,
                                    target: &selector.as_url(),
                                    repository: None,
                                    thread: None,
                                    family: Some(family),
                                    scope_key: unit.key,
                                    failure: &failure,
                                    created_at: now_utc()?,
                                },
                            )
                            .await
                            .map_err(|source| EngineError::FailureLedger {
                                original: failure.clone(),
                                source,
                            })?;
                        count_failure(&mut summary, &failure);
                        summary.completed_jobs += 1;
                        send_progress(
                            &context.progress,
                            context.run_id,
                            &summary,
                            summary.total_jobs,
                            Some(selector.as_url()),
                            progress_status(&failure),
                        );
                    }
                }
                continue;
            }
        };
        archive
            .upsert_repository_fenced(&repository, context.lease)
            .await?;
        let selector_target = selector.as_url();

        for (unit_index, unit) in units.iter().enumerate() {
            if context.cancellation.is_cancelled() {
                summary.interrupted = true;
                break;
            }
            let (started_at, sequence) = if unit_index == 0 {
                (repository_started_at, repository_sequence)
            } else {
                let started_at = now_utc()?;
                let sequence = archive
                    .reserve_observation_sequence_fenced(started_at, context.lease)
                    .await?;
                (started_at, sequence)
            };
            let since = if unit.state == ThreadListState::Closed {
                archive
                    .closed_sweep_watermark(&repository.id)
                    .await?
                    .map(overlap_start)
            } else {
                None
            };
            let job_id = archive
                .start_sync_job(
                    context.lease,
                    context.run_id,
                    &repository.id,
                    EvidenceFamily::Threads,
                    unit.key,
                    started_at,
                )
                .await?;
            archive
                .mark_scope_failures_retried(
                    context.lease,
                    &RunFailureScope {
                        run_id: context.run_id,
                        target: &selector_target,
                        family: EvidenceFamily::Threads,
                        scope_key: unit.key,
                    },
                )
                .await?;
            send_progress(
                &context.progress,
                context.run_id,
                &summary,
                summary.total_jobs,
                Some(selector.as_url()),
                SyncProgressStatus::InProgress,
            );

            let report = enumerate_repository_thread_pages(
                archive,
                client,
                ThreadScanContext {
                    repository: repository.clone(),
                    sequence,
                    started_at,
                    state: unit.state,
                    since,
                },
                Some(context.lease),
                context.cancellation,
            )
            .await?;
            summary.pages_completed = summary
                .pages_completed
                .checked_add(report.scan.pages_completed)
                .ok_or(StoreError::IntegerOutOfRange)?;
            summary.threads_seen = summary
                .threads_seen
                .checked_add(report.scan.threads_seen)
                .ok_or(StoreError::IntegerOutOfRange)?;

            let (status, failure, progress_status) = job_result(&report);
            archive
                .finish_sync_job(
                    context.lease,
                    job_id,
                    SyncJobCompletion {
                        status,
                        updated_at: now_utc()?,
                        pages_completed: report.scan.pages_completed,
                        items_committed: report.scan.threads_seen,
                        failure: failure.as_ref(),
                    },
                )
                .await?;
            if report.scan.status == RepositoryThreadScanStatus::Complete
                && unit.update_closed_watermark
            {
                archive
                    .commit_closed_sweep_watermark(
                        context.lease,
                        &repository.id,
                        sequence,
                        started_at,
                        now_utc()?,
                    )
                    .await?;
            }
            if report.scan.status == RepositoryThreadScanStatus::Complete {
                archive
                    .resolve_scope_failures(
                        context.lease,
                        &RunFailureScope {
                            run_id: context.run_id,
                            target: &selector_target,
                            family: EvidenceFamily::Threads,
                            scope_key: unit.key,
                        },
                        now_utc()?,
                    )
                    .await?;
            }
            summary.completed_jobs += 1;
            if let Some(failure) = failure.as_ref() {
                count_failure(&mut summary, failure);
            }
            if report.interrupted {
                summary.interrupted = true;
                summary.interrupted_jobs += 1;
            }
            send_progress(
                &context.progress,
                context.run_id,
                &summary,
                summary.total_jobs,
                Some(selector.as_url()),
                progress_status,
            );
            if summary.interrupted {
                break;
            }
            if context.include_comments {
                run_comment_job(archive, client, &repository, *unit, context, &mut summary).await?;
                if summary.interrupted {
                    break;
                }
            }
            run_pull_request_jobs(archive, client, &repository, *unit, context, &mut summary)
                .await?;
            if summary.interrupted {
                break;
            }
        }
        if summary.interrupted {
            break;
        }
    }
    if summary.interrupted {
        summary.pending_jobs = summary
            .total_jobs
            .saturating_sub(summary.completed_jobs)
            .saturating_add(summary.interrupted_jobs);
    }
    Ok(summary)
}
