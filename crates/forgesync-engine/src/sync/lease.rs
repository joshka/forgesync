//! # Writer lease lifetime for one sync operation
//!
//! `SyncLease` acquires the archive writer fence and owns a child cancellation token. The sync
//! coordinator asks it to create the durable run, then borrows the fence for job context.
//! Acquisition records one start timestamp, so failed run creation can release at the same time.
//!
//! `complete` polls the acquisition workflow alongside periodic lease renewal. Renewal failure
//! cancels the child token and waits for the workflow's own durable cleanup before releasing the
//! fence. This is cooperative cancellation: dropping the workflow would skip its final job writes.
//! The caller's token is never cancelled by a local lease failure.
//!
//! Release is best effort under the existing sync policy. Failure to obtain a release timestamp is
//! returned, while a store release error does not replace the operation result. This module owns
//! lease coordination only; job scope, provider traversal, and terminal run outcomes have separate
//! owners in the sync coordinator, `jobs`, and `accounting`.

use std::future::Future;
use std::time::Duration;

use forgesync_core::identity::RunId;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use forgesync_store::leases::ArchiveLeaseToken;
use tokio::time::{Instant, interval_at};
use tokio_util::sync::CancellationToken;

use super::SyncReport;
use crate::enumeration::now_utc;
use crate::error::EngineError;

/// Fence lifetime; renewal runs every third of this interval.
const LEASE_DURATION: Duration = Duration::from_secs(60);

/// Active writer capabilities retained until acquisition has finished its cleanup.
pub struct SyncLease<'a> {
    /// Archive receiving fenced job and observation writes.
    archive: &'a Archive,
    /// Fence borrowed by run creation and every job context.
    pub token: ArchiveLeaseToken,
    /// Timestamp shared by lease acquisition and durable run creation.
    pub started_at: UtcTimestamp,
    /// Child token used to stop jobs when renewal fails without cancelling the caller.
    pub cancellation: CancellationToken,
}

impl<'a> SyncLease<'a> {
    /// Acquires the writer fence before any run or provider acquisition is started.
    pub async fn acquire(
        archive: &'a Archive,
        cancellation: &CancellationToken,
    ) -> Result<Self, EngineError> {
        let started_at = now_utc()?;
        let token = archive
            .acquire_archive_lease(started_at, LEASE_DURATION)
            .await?;
        Ok(Self {
            archive,
            token,
            started_at,
            cancellation: cancellation.child_token(),
        })
    }

    /// Creates the durable run under this fence; failed creation releases before returning its
    /// cause.
    pub async fn start_run(
        &self,
        parent: Option<RunId>,
        scope: &serde_json::Value,
    ) -> Result<RunId, EngineError> {
        match self
            .archive
            .create_run(&self.token, parent, self.started_at, scope)
            .await
        {
            Ok(run_id) => Ok(run_id),
            Err(error) => {
                self.abandon().await;
                Err(error.into())
            }
        }
    }

    /// Releases a fence whose run could not be created, preserving the original creation error.
    async fn abandon(&self) {
        let _ = self
            .archive
            .release_archive_lease(&self.token, self.started_at)
            .await;
    }

    /// Maintains ownership through workflow cleanup, then attempts release on success or failure.
    pub async fn complete(
        &self,
        operation: impl Future<Output = Result<SyncReport, EngineError>>,
    ) -> Result<SyncReport, EngineError> {
        let result = self.maintain(operation).await;
        let release_at = now_utc()?;
        let _ = self
            .archive
            .release_archive_lease(&self.token, release_at)
            .await;
        result
    }

    /// Renews the fence while polling acquisition; failed renewal drains cooperative cleanup.
    async fn maintain(
        &self,
        operation: impl Future<Output = Result<SyncReport, EngineError>>,
    ) -> Result<SyncReport, EngineError> {
        let mut operation = Box::pin(operation);
        let interval = LEASE_DURATION / 3;
        let mut heartbeat = interval_at(Instant::now() + interval, interval);
        loop {
            tokio::select! {
                result = &mut operation => return result,
                _ = heartbeat.tick() => {
                    if let Err(error) = self.renew().await {
                        self.cancellation.cancel();
                        let _ = operation.await;
                        return Err(error);
                    }
                }
            }
        }
    }

    /// Uses the current clock to extend this exact fence, propagating clock or ownership failure.
    async fn renew(&self) -> Result<(), EngineError> {
        let now = now_utc()?;
        self.archive
            .heartbeat_archive_lease(&self.token, now, LEASE_DURATION)
            .await?;
        Ok(())
    }
}
