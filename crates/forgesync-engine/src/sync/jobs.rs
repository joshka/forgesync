//! # Repository lookup and durable parent-thread jobs
//!
//! `run_jobs` visits selected repository scopes and returns the accumulated run report.
//! `RepositorySync` resolves a provider repository and isolates lookup failures across its selected
//! families. A successful lookup yields `RepositoryWork` values for individual thread-state scopes.
//!
//! `ThreadJob` owns a started parent-thread job and its scan context. The first scan reuses the
//! sequence reserved before repository lookup; subsequent scans reserve their own acquisition.
//! Completion writes the ledger, advances a closed-sweep watermark only for complete coverage,
//! resolves satisfied failures, then publishes progress. Child-family jobs follow that parent scan
//! and keep their own failure/completeness boundaries.

use std::collections::HashMap;

use forgesync_core::content::Repository;
use forgesync_core::coverage::{EvidenceFamily, Failure};
use forgesync_core::identity::{GitHubHost, ObservationSequence};
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_github::error::GitHubError;
use forgesync_github::resources::{ThreadListState, fetch_repository};
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::runs::RunFailureInput;

use super::accounting::WorkSummary;
use super::repository_work::RepositoryWork;
use super::support::{overlap_start, progress_status};
use super::thread_job::ThreadJob;
use super::{ScopeUnit, SyncProgressStatus, SyncRunContext};
use crate::clock::now_utc;
use crate::enumeration::ThreadScanContext;
use crate::error::EngineError;
use crate::provider_failure::github_failure;
use crate::reference::RepositorySelector;

/// Visits repositories in request order, preserving committed work when cancellation stops a run.
pub async fn run_jobs(
    archive: &Archive,
    clients: &HashMap<GitHubHost, GitHubClient>,
    selectors: &[RepositorySelector],
    units: &[ScopeUnit],
    context: &SyncRunContext<'_>,
) -> Result<WorkSummary, EngineError> {
    let mut summary = WorkSummary {
        total_jobs: context.total_jobs,
        ..Default::default()
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
        let sync = RepositorySync {
            archive,
            client,
            selector,
            context,
        };
        sync.run(units, &mut summary).await?;
        if summary.interrupted {
            break;
        }
    }
    summary.finish_pending();
    Ok(summary)
}

/// Provider lookup scope and immutable services for one repository's jobs.
struct RepositorySync<'a> {
    /// Archive receiving fenced repository writes, reservations, and failure-ledger entries.
    archive: &'a Archive,
    /// Provider client selected for the repository's host before lookup begins.
    client: &'a GitHubClient,
    /// Requested owner/name identity used for lookup and diagnostics before a provider ID exists.
    selector: &'a RepositorySelector,
    /// Shared run ID, lease, cancellation, selected families, and progress destination.
    context: &'a SyncRunContext<'a>,
}

impl RepositorySync<'_> {
    /// Resolves the repository once, then executes each selected thread-state scope.
    async fn run(&self, units: &[ScopeUnit], summary: &mut WorkSummary) -> Result<(), EngineError> {
        let first = Acquisition::reserve(self.archive, self.context).await?;
        let Some(repository) = self.repository(units, summary).await? else {
            return Ok(());
        };
        self.archive
            .upsert_repository_fenced(&repository, self.context.lease)
            .await?;
        for (index, unit) in units.iter().enumerate() {
            if self.context.cancellation.is_cancelled() {
                summary.interrupted = true;
                break;
            }
            let acquisition = if index == 0 {
                first
            } else {
                Acquisition::reserve(self.archive, self.context).await?
            };
            let work = RepositoryWork {
                archive: self.archive,
                client: self.client,
                repository: &repository,
                unit: *unit,
                context: self.context,
            };
            let scan = self.scan(&repository, *unit, acquisition).await?;
            let job = ThreadJob::start(work, scan, summary).await?;
            job.run(summary).await?;
            if summary.interrupted {
                break;
            }
            self.children(work, summary).await?;
            if summary.interrupted {
                break;
            }
        }
        Ok(())
    }

    /// Converts repository lookup failure into scoped job failures without aborting siblings.
    async fn repository(
        &self,
        units: &[ScopeUnit],
        summary: &mut WorkSummary,
    ) -> Result<Option<Repository>, EngineError> {
        match fetch_repository(
            self.client,
            self.selector.host(),
            self.selector.owner(),
            self.selector.name(),
            self.context.cancellation,
        )
        .await
        {
            Ok(repository) => Ok(Some(repository)),
            Err(GitHubError::Cancelled) => {
                summary.interrupted = true;
                Ok(None)
            }
            Err(error) => self.lookup_failed(error, units, summary).await,
        }
    }

    /// Attributes an unavailable repository to each selected parent/comment family and scope.
    async fn lookup_failed(
        &self,
        error: GitHubError,
        units: &[ScopeUnit],
        summary: &mut WorkSummary,
    ) -> Result<Option<Repository>, EngineError> {
        let failure = github_failure(&error);
        for unit in units {
            self.record_lookup_failure(*unit, EvidenceFamily::Threads, &failure, summary)
                .await?;
            if self.context.include_comments {
                self.record_lookup_failure(*unit, EvidenceFamily::Comments, &failure, summary)
                    .await?;
            }
        }
        Ok(None)
    }

    /// Writes one lookup failure before updating the run's completed-job accounting.
    async fn record_lookup_failure(
        &self,
        unit: ScopeUnit,
        family: EvidenceFamily,
        failure: &Failure,
        summary: &mut WorkSummary,
    ) -> Result<(), EngineError> {
        let target = self.selector.as_url();
        let input = RunFailureInput {
            run_id: self.context.run_id,
            target: &target,
            repository: None,
            thread: None,
            family: Some(family),
            scope_key: unit.key,
            failure,
            created_at: now_utc()?,
        };
        self.archive
            .record_run_failure(self.context.lease, input)
            .await
            .map_err(|source| EngineError::FailureLedger {
                original: failure.clone(),
                source,
            })?;
        summary.record_failure(failure);
        summary.completed_jobs += 1;
        self.publish(summary, progress_status(failure));
        Ok(())
    }

    /// Builds the provider scan scope, applying overlap only to closed-thread sweeps.
    async fn scan(
        &self,
        repository: &Repository,
        unit: ScopeUnit,
        acquisition: Acquisition,
    ) -> Result<ThreadScanContext, EngineError> {
        let since = if unit.state == ThreadListState::Closed {
            self.archive
                .closed_sweep_watermark(&repository.id)
                .await?
                .map(overlap_start)
        } else {
            None
        };
        Ok(ThreadScanContext {
            repository: repository.clone(),
            sequence: acquisition.sequence,
            started_at: acquisition.started_at,
            state: unit.state,
            since,
        })
    }

    /// Runs selected child families after the parent scan, stopping on interruption.
    async fn children(
        &self,
        work: RepositoryWork<'_>,
        summary: &mut WorkSummary,
    ) -> Result<(), EngineError> {
        if self.context.include_comments {
            work.sync_comments(summary).await?;
        }
        if !summary.interrupted {
            work.sync_pull_requests(summary).await?;
        }
        Ok(())
    }

    /// Publishes the repository's current accounting after a durable update.
    fn publish(&self, summary: &WorkSummary, status: SyncProgressStatus) {
        self.context
            .publish(summary, Some(self.selector.as_url()), status);
    }
}

/// Timestamp and local order reserved together before provider acquisition.
#[derive(Clone, Copy)]
struct Acquisition {
    /// Local reservation time used as the scan start, independent of provider update timestamps.
    started_at: UtcTimestamp,
    /// Archive ordering token reserved before provider I/O, not the order responses arrive.
    sequence: ObservationSequence,
}

impl Acquisition {
    /// Establishes the scan's local order before repository or page requests can finish.
    async fn reserve(archive: &Archive, context: &SyncRunContext<'_>) -> Result<Self, EngineError> {
        let started_at = now_utc()?;
        let sequence = archive
            .reserve_observation_sequence_fenced(started_at, context.lease)
            .await?;
        Ok(Self {
            started_at,
            sequence,
        })
    }
}
