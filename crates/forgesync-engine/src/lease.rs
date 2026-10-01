//! Writer-lease lifetime shared by every fenced engine workflow.

use std::future::Future;
use std::time::Duration;

use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::leases::ArchiveLeaseToken;
use tokio::time::{Instant, interval_at};
use tokio_util::sync::CancellationToken;

use crate::clock::now_utc;
use crate::error::EngineError;

/// Runs `operation` while holding the archive writer lease, renewing it every third of `duration`.
///
/// The operation receives the fence and a child of `cancellation`. Renewal failure cancels only
/// that child and waits for the operation's own durable cleanup before returning the renewal error;
/// the caller's token is never cancelled. The lease is released afterwards in every case. A failed
/// release is only logged: every write was fenced, so the operation's result (including a partial
/// report) is still accurate, and the lease expires on its own.
pub async fn with_writer_lease<T>(
    archive: &Archive,
    duration: Duration,
    cancellation: &CancellationToken,
    operation: impl AsyncFnOnce(&ArchiveLeaseToken, &CancellationToken) -> Result<T, EngineError>,
) -> Result<T, EngineError> {
    let token = archive.acquire_archive_lease(now_utc()?, duration).await?;
    let child = cancellation.child_token();
    let renewal = renew_until_failure(archive, &token, duration);
    let result = maintain(operation(&token, &child), renewal, &child).await;
    if let Err(error) = release(archive, &token).await {
        tracing::warn!(code = error.code(), "archive writer lease release failed");
    }
    result
}

/// Polls the operation and renewal together.
///
/// Renewal may wait for the sole writer connection while the operation's transaction still holds
/// it; awaiting renewal alone inside a select branch would stop that transaction and time out the
/// pool. On renewal failure the operation is cancelled cooperatively and drained, not dropped.
async fn maintain<T>(
    operation: impl Future<Output = Result<T, EngineError>>,
    renewal: impl Future<Output = EngineError>,
    child: &CancellationToken,
) -> Result<T, EngineError> {
    let mut operation = std::pin::pin!(operation);
    tokio::select! {
        result = &mut operation => result,
        error = renewal => {
            child.cancel();
            let _ = operation.await;
            Err(error)
        }
    }
}

/// Heartbeats the fence every third of `duration`; resolves only with the first renewal error.
async fn renew_until_failure(
    archive: &Archive,
    token: &ArchiveLeaseToken,
    duration: Duration,
) -> EngineError {
    let period = duration / 3;
    let mut heartbeat = interval_at(Instant::now() + period, period);
    loop {
        heartbeat.tick().await;
        let renewed = match now_utc() {
            Ok(now) => archive.heartbeat_archive_lease(token, now, duration).await,
            Err(error) => return error,
        };
        if let Err(error) = renewed {
            return error.into();
        }
    }
}

/// Releases the exact fence; `false` from the store means another owner already holds it.
async fn release(archive: &Archive, token: &ArchiveLeaseToken) -> Result<(), EngineError> {
    if archive.release_archive_lease(token, now_utc()?).await? {
        Ok(())
    } else {
        Err(StoreError::ArchiveLeaseLost.into())
    }
}

#[cfg(test)]
#[path = "lease/tests.rs"]
mod tests;
