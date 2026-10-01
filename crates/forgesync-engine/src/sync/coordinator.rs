//! Coordinate one fenced sync run.
//!
//! A provider failure remains a structured job outcome; a fatal coordination/archive failure
//! attempts a terminal failed-run record while preserving the original error. Committed source
//! observations survive later job failure. No store transaction is held across provider I/O.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use forgesync_core::coverage::{Failure, FailureKind};
use forgesync_core::identity::{GitHubHost, RunId};
use forgesync_core::outcome::OperationOutcome;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::leases::ArchiveLeaseToken;
use forgesync_store::runs::RunFailureInput;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::clock::now_utc;
use crate::error::EngineError;
use crate::lease::with_writer_lease;
use crate::reference::RepositorySelector;
use crate::sync::accounting::WorkSummary;
use crate::sync::repository::{RepositorySync, ScopeUnit};
use crate::sync::{RunScope, SyncProgress, SyncProgressStatus, SyncReport, SyncRequest};

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
    let selectors = request.repositories(archive, clients).await?;
    let units = ScopeUnit::expand(request.scope);
    let scope = RunScope {
        repositories: selectors.iter().map(RepositorySelector::as_url).collect(),
        all: request.all,
        thread_scope: request.scope,
        include_comments: request.include_comments,
        include_reviews: request.include_reviews,
        include_review_threads: request.include_review_threads,
    };
    let scope = serde_json::to_value(scope).map_err(StoreError::from)?;
    with_writer_lease(
        archive,
        LEASE_DURATION,
        cancellation,
        async |lease, cancellation| {
            let id = archive
                .create_run(lease, request.parent_run, now_utc()?, &scope)
                .await?;
            let run = Run {
                archive,
                id,
                lease,
                cancellation,
                progress,
                request,
            };
            run.execute(clients, &selectors, &units).await
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
        if self.all == !self.repositories.is_empty() {
            return Err(EngineError::InvalidSyncScope);
        }
        let selectors = if self.all {
            archive
                .list_repositories()
                .await?
                .iter()
                .map(RepositorySelector::from_repository)
                .collect()
        } else {
            self.repositories.clone()
        };
        let mut seen = HashSet::new();
        let mut unique = Vec::with_capacity(selectors.len());
        for selector in selectors {
            if seen.insert(selector.clone()) {
                client_for(clients, &selector)?;
                unique.push(selector);
            }
        }
        Ok(unique)
    }
}

/// Finds the provider client configured for the selector's host.
fn client_for<'a>(
    clients: &'a HashMap<GitHubHost, GitHubClient>,
    selector: &RepositorySelector,
) -> Result<&'a GitHubClient, EngineError> {
    clients
        .get(selector.host())
        .ok_or_else(|| EngineError::GitHubClientMissing {
            host: selector.host().as_str().to_owned(),
        })
}

/// Run identity, writer fence, cancellation, and selected families shared by every job.
pub struct Run<'a> {
    pub archive: &'a Archive,
    pub id: RunId,
    pub lease: &'a ArchiveLeaseToken,
    /// Child token cancelled by the caller or a failed lease renewal.
    pub cancellation: &'a CancellationToken,
    progress: Option<mpsc::Sender<SyncProgress>>,
    pub request: &'a SyncRequest,
}

impl Run<'_> {
    /// Runs selected jobs and persists the terminal run outcome.
    async fn execute(
        &self,
        clients: &HashMap<GitHubHost, GitHubClient>,
        selectors: &[RepositorySelector],
        units: &[ScopeUnit],
    ) -> Result<SyncReport, EngineError> {
        let jobs_per_unit = 1 + u64::from(self.request.include_comments);
        let mut summary = WorkSummary {
            total_jobs: selectors.len() as u64 * units.len() as u64 * jobs_per_unit,
            ..WorkSummary::default()
        };
        if let Err(error) = self.visit(clients, selectors, units, &mut summary).await {
            return self.fail(error).await;
        }
        summary.finish_pending();
        let outcome = summary.outcome();
        self.archive
            .finish_run(self.lease, self.id, now_utc()?, &outcome)
            .await?;
        let detail = self
            .archive
            .run_detail(self.id)
            .await?
            .ok_or(StoreError::RunMissing)?;
        Ok(SyncReport {
            run: detail.run,
            jobs: detail.jobs,
            failures: detail.failures,
            repositories_selected: selectors.len() as u64,
            completed_jobs: summary.completed_jobs,
            total_jobs: summary.total_jobs,
            failed_jobs: summary.failed_jobs,
            deferred_jobs: summary.deferred_jobs,
            pages_completed: summary.pages_completed,
            threads_seen: summary.threads_seen,
            comments_seen: summary.comments_seen,
            pull_request_metadata_seen: summary.pull_request_metadata_seen,
            reviews_seen: summary.reviews_seen,
            review_threads_seen: summary.review_threads_seen,
            outcome,
        })
    }

    /// Visits repositories in request order, stopping (with committed work kept) on interruption.
    async fn visit(
        &self,
        clients: &HashMap<GitHubHost, GitHubClient>,
        selectors: &[RepositorySelector],
        units: &[ScopeUnit],
        summary: &mut WorkSummary,
    ) -> Result<(), EngineError> {
        for selector in selectors {
            if self.cancellation.is_cancelled() {
                summary.interrupted = true;
                return Ok(());
            }
            let client = client_for(clients, selector)?;
            RepositorySync::run(self, client, selector, units, summary).await?;
            if summary.interrupted {
                return Ok(());
            }
        }
        Ok(())
    }

    /// Attempts a terminal failure record after acquisition loses its work summary, preserving the
    /// original error even if the clock or failure write also fails.
    async fn fail(&self, original: EngineError) -> Result<SyncReport, EngineError> {
        let outcome = OperationOutcome::Failed {
            failure: Failure {
                kind: FailureKind::Archive,
                message: "sync stopped before its work summary could be persisted".to_owned(),
            },
        };
        if let Ok(finished_at) = now_utc() {
            let _ = self
                .archive
                .finish_run(self.lease, self.id, finished_at, &outcome)
                .await;
        }
        Err(original)
    }

    /// Writes a provider failure to the run ledger, retaining it if the ledger write fails.
    pub async fn record_failure(&self, input: RunFailureInput<'_>) -> Result<(), EngineError> {
        let failure = input.failure.clone();
        self.archive
            .record_run_failure(self.lease, input)
            .await
            .map_err(|source| EngineError::FailureLedger {
                original: failure,
                source,
            })
    }

    /// Publishes current accounting without blocking acquisition.
    pub fn publish(&self, summary: &WorkSummary, repository: &str, status: SyncProgressStatus) {
        if let Some(sender) = &self.progress {
            let _ = sender.try_send(SyncProgress {
                run_id: self.id,
                completed_jobs: summary.completed_jobs,
                total_jobs: summary.total_jobs,
                threads_seen: summary.threads_seen,
                comments_seen: summary.comments_seen,
                pull_request_metadata_seen: summary.pull_request_metadata_seen,
                reviews_seen: summary.reviews_seen,
                review_threads_seen: summary.review_threads_seen,
                repository: Some(repository.to_owned()),
                status,
            });
        }
    }
}
