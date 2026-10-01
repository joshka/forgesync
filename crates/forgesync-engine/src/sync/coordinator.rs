//! # Coordinate one fenced sync run
//!
//! `sync_repositories` validates repository selection before writer acquisition, expands the
//! requested thread scopes, and starts one durable run. It then delegates individual repository
//! and family jobs while `SyncLease` renews authorization and handles cooperative cancellation.
//!
//! `SyncRequest` methods normalize selected repositories, count initial jobs, and preserve scope
//! for inspection/retry. Pull-request family discovery can extend that initial count later.
//! `execute_and_finalize` writes the terminal run outcome and reads back its persisted report.
//!
//! A provider failure remains a structured job outcome; a fatal coordination/archive failure
//! attempts a terminal failed-run record while preserving the original error. Committed source
//! observations survive later job failure. The lease owner waits for operation cleanup before
//! release; this module never holds a store transaction across provider I/O.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use forgesync_core::coverage::{Failure, FailureKind};
use forgesync_core::identity::GitHubHost;
use forgesync_core::outcome::OperationOutcome;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::clock::now_utc;
use crate::error::EngineError;
use crate::lease::with_writer_lease;
use crate::reference::RepositorySelector;
use crate::sync::jobs::run_jobs;
use crate::sync::scope::{ScopeUnit, SyncRunContext, units};
use crate::sync::support::resolve_selectors;
use crate::sync::{SyncProgress, SyncReport, SyncRequest};

/// Writer lease lifetime, renewed every third of this interval.
const LEASE_DURATION: Duration = Duration::from_secs(60);

/// Runs a fenced, resumable sync and returns a durable partial or complete report.
///
/// Progress updates are snapshots sent with `try_send`; a full or disconnected channel never
/// blocks archive writes or changes the final report.
pub async fn sync_repositories(
    archive: &Archive,
    clients: &HashMap<GitHubHost, GitHubClient>,
    request: &SyncRequest,
    cancellation: &CancellationToken,
    progress: Option<mpsc::Sender<SyncProgress>>,
) -> Result<SyncReport, EngineError> {
    let unique_selectors = request.repositories(archive, clients).await?;

    let units = units(request.scope);
    let total_jobs = request.initial_jobs(unique_selectors.len(), units.len())?;
    let run_scope = request.run_scope(&unique_selectors);
    with_writer_lease(
        archive,
        LEASE_DURATION,
        cancellation,
        async |lease, cancellation| {
            let run_id = archive
                .create_run(lease, request.parent_run, now_utc()?, &run_scope)
                .await?;
            let context = SyncRunContext {
                total_jobs,
                include_comments: request.include_comments,
                include_reviews: request.include_reviews,
                include_review_threads: request.include_review_threads,
                run_id,
                lease,
                cancellation,
                progress,
            };
            execute_and_finalize(archive, clients, &unique_selectors, &units, context).await
        },
    )
    .await
}

impl SyncRequest {
    /// Resolves unique repositories in request order and verifies a client exists for every host.
    /// Invalid selection fails before acquiring the archive writer fence or creating a run.
    async fn repositories(
        &self,
        archive: &Archive,
        clients: &HashMap<GitHubHost, GitHubClient>,
    ) -> Result<Vec<RepositorySelector>, EngineError> {
        if (self.all && !self.repositories.is_empty())
            || (!self.all && self.repositories.is_empty())
        {
            return Err(EngineError::InvalidSyncScope);
        }

        let selectors = resolve_selectors(archive, self).await?;
        let mut unique_selectors = Vec::with_capacity(selectors.len());
        let mut seen = HashSet::new();
        for selector in selectors {
            if seen.insert(selector.clone()) {
                if !clients.contains_key(selector.host()) {
                    return Err(EngineError::GitHubClientMissing {
                        host: selector.host().as_str().to_owned(),
                    });
                }
                unique_selectors.push(selector);
            }
        }

        Ok(unique_selectors)
    }

    /// Counts initial parent/comment jobs before a lease is acquired; pull-request jobs are added
    /// later only for scopes containing eligible pull requests.
    fn initial_jobs(&self, repositories: usize, scopes: usize) -> Result<u64, EngineError> {
        let jobs = repositories
            .checked_mul(scopes)
            .and_then(|count| count.checked_mul(1 + usize::from(self.include_comments)))
            .and_then(|count| u64::try_from(count).ok())
            .ok_or(StoreError::IntegerOutOfRange)?;
        Ok(jobs)
    }

    /// Records normalized selected repositories and original policy for durable inspection/retry.
    fn run_scope(&self, repositories: &[RepositorySelector]) -> serde_json::Value {
        let repositories = repositories
            .iter()
            .map(RepositorySelector::as_url)
            .collect::<Vec<_>>();
        json!({
            "repositories": repositories,
            "all": self.all,
            "thread_scope": self.scope,
            "include_comments": self.include_comments,
            "include_reviews": self.include_reviews,
            "include_review_threads": self.include_review_threads,
        })
    }
}

/// Runs selected jobs and persists the terminal run outcome.
async fn execute_and_finalize(
    archive: &Archive,
    clients: &HashMap<GitHubHost, GitHubClient>,
    selectors: &[RepositorySelector],
    units: &[ScopeUnit],
    context: SyncRunContext<'_>,
) -> Result<SyncReport, EngineError> {
    let work = match run_jobs(archive, clients, selectors, units, &context).await {
        Ok(work) => work,
        Err(error) => return context.fail_run(archive, error).await,
    };
    let outcome = work.outcome();
    archive
        .finish_run(context.lease, context.run_id, now_utc()?, &outcome)
        .await?;
    let detail = archive
        .run_detail(context.run_id)
        .await?
        .ok_or(StoreError::RunMissing)?;
    Ok(SyncReport {
        run: detail.run,
        jobs: detail.jobs,
        failures: detail.failures,
        repositories_selected: u64::try_from(selectors.len())
            .map_err(|_| StoreError::IntegerOutOfRange)?,
        completed_jobs: work.completed_jobs,
        total_jobs: work.total_jobs,
        failed_jobs: work.failed_jobs,
        deferred_jobs: work.deferred_jobs,
        pages_completed: work.pages_completed,
        threads_seen: work.threads_seen,
        comments_seen: work.comments_seen,
        pull_request_metadata_seen: work.pull_request_metadata_seen,
        reviews_seen: work.reviews_seen,
        review_threads_seen: work.review_threads_seen,
        outcome,
    })
}

impl SyncRunContext<'_> {
    /// Attempts a terminal failure record after acquisition loses its work summary, preserving the
    /// original error even if the clock or failure write also fails.
    async fn fail_run(
        &self,
        archive: &Archive,
        original_error: EngineError,
    ) -> Result<SyncReport, EngineError> {
        let failure = Failure {
            kind: FailureKind::Archive,
            message: "sync stopped before its work summary could be persisted".to_owned(),
        };
        let outcome = OperationOutcome::Failed { failure };
        if let Ok(finished_at) = now_utc() {
            let _ = archive
                .finish_run(self.lease, self.run_id, finished_at, &outcome)
                .await;
        }
        Err(original_error)
    }
}
