//! # Cluster writer release and cooperative interruption
//!
//! Completion must release the archive fence even when generation construction fails. The named
//! failure cases reclaim at an epoch timestamp: a leaked current lease cannot expire merely because
//! the test ran slowly. These cases use an on-disk archive and do not build a candidate graph.
//!
//! The interruption case isolates the child cancellation contract. Its operation waits for the
//! child token, records cleanup, and returns a different error. The triggering failure must win,
//! cleanup must finish, and the caller's cancellation token must remain untouched.
//!
//! Generation integration cases establish membership and retirement behavior separately. These
//! tests make the lease owner safe to change without rebuilding those larger scenarios mentally.
//! Already-released cases establish that cleanup failure rejects success while preserving an
//! earlier operation failure. Each case invokes the same real completion operation with explicit
//! input.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use rstest::rstest;
use tokio_util::sync::CancellationToken;

use crate::clustering::lease::{ClusterBuildLease, finish_cluster_lease_result};
use crate::error::EngineError;

/// Distinguishes fixture directories within this process without provider or clock dependence.
static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[rstest]
#[case::invalid_input(EngineError::InvalidClusterInput)]
#[case::cancelled_build(EngineError::ClusteringCancelled)]
#[tokio::test]
async fn failed_completion_preserves_the_error_and_releases_the_fence(#[case] error: EngineError) {
    let directory = archive_directory();
    let archive = Archive::create(directory.join("archive.sqlite"))
        .await
        .expect("create archive");
    let cancellation = CancellationToken::new();
    let lease = ClusterBuildLease::acquire(&archive, &cancellation)
        .await
        .expect("acquire lease");
    let expected_code = error.code();

    let result = lease.complete(async { Err(error) }, &cancellation).await;

    assert_eq!(result.expect_err("failed operation").code(), expected_code);
    let epoch = UtcTimestamp::parse("1970-01-01T00:00:00Z").expect("epoch timestamp");
    let reclaimed = archive
        .acquire_archive_lease(epoch, Duration::from_secs(60))
        .await
        .expect("previous fence released");
    archive
        .release_archive_lease(&reclaimed, epoch)
        .await
        .expect("release test fence");
    archive.close().await;
    std::fs::remove_dir_all(directory).expect("remove fixture directory");
}

#[tokio::test]
async fn interruption_waits_for_child_cleanup_without_cancelling_the_caller() {
    let directory = archive_directory();
    let archive = Archive::create(directory.join("archive.sqlite"))
        .await
        .expect("create archive");
    let caller = CancellationToken::new();
    let lease = ClusterBuildLease::acquire(&archive, &caller)
        .await
        .expect("acquire lease");
    let mut cleaned_up = false;
    let operation = async {
        lease.cancellation.cancelled().await;
        cleaned_up = true;
        Err(EngineError::ClusteringCancelled)
    };
    let mut operation = Box::pin(operation);

    let result = lease
        .interrupt(operation.as_mut(), EngineError::InvalidClusterInput)
        .await;
    drop(operation);

    assert!(matches!(result, Err(EngineError::InvalidClusterInput)));
    assert!(cleaned_up);
    assert!(lease.cancellation.is_cancelled());
    assert!(!caller.is_cancelled());
    archive
        .release_archive_lease(
            &lease.token,
            UtcTimestamp::parse("1970-01-01T00:00:00Z").expect("epoch"),
        )
        .await
        .expect("release fixture lease");
    archive.close().await;
    std::fs::remove_dir_all(directory).expect("remove fixture directory");
}

#[rstest]
#[case::success_requires_release(Ok(()), "archive_lease_lost")]
#[case::operation_error_wins(Err(EngineError::InvalidClusterInput), "cluster_input_invalid")]
#[tokio::test]
async fn already_released_fence_preserves_completion_error_precedence(
    #[case] operation: Result<(), EngineError>,
    #[case] expected_code: &str,
) {
    let directory = archive_directory();
    let archive = Archive::create(directory.join("archive.sqlite"))
        .await
        .expect("create archive");
    let cancellation = CancellationToken::new();
    let lease = ClusterBuildLease::acquire(&archive, &cancellation)
        .await
        .expect("acquire lease");
    let epoch = UtcTimestamp::parse("1970-01-01T00:00:00Z").expect("epoch timestamp");
    let released = archive
        .release_archive_lease(&lease.token, epoch)
        .await
        .expect("release fixture fence before completion");
    assert!(released);

    let result = finish_cluster_lease_result(&archive, &lease.token, operation).await;

    assert_eq!(result.expect_err("completion fails").code(), expected_code);
    archive.close().await;
    std::fs::remove_dir_all(directory).expect("remove fixture directory");
}

/// Creates a unique directory containing only this test's archive and SQLite sidecars.
fn archive_directory() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "forgesync-cluster-lease-{}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).expect("create fixture directory");
    directory
}
