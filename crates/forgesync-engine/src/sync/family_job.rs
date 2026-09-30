//! # Progress and terminal state of a durable sync-family job
//!
//! A `FamilyJob` joins the persisted job ID with the progress accumulated from its thread
//! attempts. `FamilyJobProgress` keeps successful page/member counts separate from hard failures,
//! deferred failures, and interruption. A sibling thread's success survives another's failure.
//!
//! Accumulation chooses the first failure in each category. Terminal selection gives cancellation
//! precedence, then hard failure, then deferral. `finish` consumes the job, writes its final
//! status, updates the run summary, and publishes progress. Source observations remain
//! independently committed by the acquisition modules; job accounting never rewrites their
//! evidence.

use forgesync_core::coverage::{Failure, FailureKind};
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::runs::{SyncJobCompletion, SyncJobStatus};

use super::SyncProgressStatus;
use super::accounting::WorkSummary;
use super::support::progress_status;
use crate::clock::now_utc;
use crate::error::EngineError;
use crate::sync::scope::{SyncRunContext, ThreadFamilyResult};

/// A started durable job together with the thread outcomes it has accumulated.
pub struct FamilyJob {
    /// Store-assigned ID used for the terminal write.
    pub id: i64,
    /// Counts and failures gathered before terminal status is selected.
    pub progress: FamilyJobProgress,
}

impl FamilyJob {
    /// Starts local accounting for a job already created in the archive.
    pub fn new(id: i64) -> Self {
        Self {
            id,
            progress: FamilyJobProgress::default(),
        }
    }

    /// Persists one family job's terminal state before moving to the next.
    pub async fn finish(
        self,
        archive: &Archive,
        context: &SyncRunContext<'_>,
        summary: &mut WorkSummary,
        repository: &str,
    ) -> Result<(), EngineError> {
        let (status, failure, progress_status) = self.progress.outcome();
        archive
            .finish_sync_job(
                context.lease,
                self.id,
                SyncJobCompletion {
                    status,
                    updated_at: now_utc()?,
                    pages_completed: self.progress.pages_completed,
                    items_committed: self.progress.items_committed,
                    failure,
                },
            )
            .await?;
        summary.record_family_outcome(failure, self.progress.interrupted);
        context.publish(summary, Some(repository.to_owned()), progress_status);
        Ok(())
    }
}

/// Aggregated thread outcomes before a family job reaches a terminal state.
#[derive(Default)]
pub struct FamilyJobProgress {
    /// Pages durably staged by attempted threads.
    pub pages_completed: u64,
    /// Members applied by completed family observations.
    pub items_committed: u64,
    /// First non-rate-limited provider failure, preferred over deferral.
    pub hard_failure: Option<Failure>,
    /// First rate-limit failure when no harder failure prevents completion.
    pub deferred_failure: Option<Failure>,
    /// Whether any selected work was cancelled before finishing.
    pub interrupted: bool,
}

impl FamilyJobProgress {
    /// Selects interruption, hard failure, deferral, or completion without discarding counts.
    pub fn outcome(&self) -> (SyncJobStatus, Option<&Failure>, SyncProgressStatus) {
        if self.interrupted {
            (
                SyncJobStatus::Interrupted,
                None,
                SyncProgressStatus::Interrupted,
            )
        } else if let Some(failure) = self.hard_failure.as_ref() {
            (
                SyncJobStatus::Failed,
                Some(failure),
                progress_status(failure),
            )
        } else if let Some(failure) = self.deferred_failure.as_ref() {
            (
                SyncJobStatus::Deferred,
                Some(failure),
                progress_status(failure),
            )
        } else {
            (SyncJobStatus::Complete, None, SyncProgressStatus::Complete)
        }
    }

    /// Adds a thread-family result to aggregate job counts and failures.
    pub fn accumulate<T>(&mut self, result: &ThreadFamilyResult<T>) -> Result<(), StoreError> {
        self.pages_completed = self
            .pages_completed
            .checked_add(result.pages_completed)
            .ok_or(StoreError::IntegerOutOfRange)?;
        self.items_committed = self
            .items_committed
            .checked_add(result.items_committed)
            .ok_or(StoreError::IntegerOutOfRange)?;
        self.interrupted |= result.interrupted;
        if let Some(failure) = result.failure.as_ref() {
            let failure_slot = if failure.kind == FailureKind::RateLimited {
                &mut self.deferred_failure
            } else {
                &mut self.hard_failure
            };
            if failure_slot.is_none() {
                *failure_slot = Some(failure.clone());
            }
        }
        Ok(())
    }
}
