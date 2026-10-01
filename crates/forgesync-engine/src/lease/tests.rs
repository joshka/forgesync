//! Lease renewal must leave the active workflow runnable, drain cleanup on failure, and always
//! release the fence. Renewal fixtures announce that they wait for a writer connection and then
//! stay pending, reproducing the sole-writer scheduling dependency without SQL timing.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use rstest::rstest;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::error::EngineError;
use crate::lease::{maintain, with_writer_lease};

const LEASE: Duration = Duration::from_secs(60);

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[tokio::test]
async fn operation_continues_while_renewal_waits_for_its_connection() {
    let child = CancellationToken::new();
    let waiting = Notify::new();
    let operation = async {
        waiting.notified().await;
        Err::<(), _>(EngineError::InvalidSyncScope)
    };
    let renewal = async {
        waiting.notify_one();
        std::future::pending::<EngineError>().await
    };

    let result = tokio::time::timeout(Duration::from_secs(1), maintain(operation, renewal, &child))
        .await
        .expect("operation is polled while renewal waits");

    assert!(matches!(result, Err(EngineError::InvalidSyncScope)));
    assert!(!child.is_cancelled());
}

#[tokio::test]
async fn renewal_failure_drains_cleanup_without_cancelling_the_caller() {
    let caller = CancellationToken::new();
    let child = caller.child_token();
    let mut cleaned_up = false;
    let operation = async {
        child.cancelled().await;
        cleaned_up = true;
        Err::<(), _>(EngineError::InvalidSyncScope)
    };

    let result = maintain(
        operation,
        async { StoreError::ArchiveLeaseLost.into() },
        &child,
    )
    .await;

    assert!(matches!(
        result,
        Err(EngineError::Store(StoreError::ArchiveLeaseLost))
    ));
    assert!(cleaned_up);
    assert!(child.is_cancelled());
    assert!(!caller.is_cancelled());
}

#[rstest]
#[case::invalid_input(EngineError::InvalidClusterInput)]
#[case::cancelled(EngineError::Cancelled)]
#[tokio::test]
async fn failed_operation_preserves_the_error_and_releases_the_fence(#[case] error: EngineError) {
    let (directory, archive) = fixture_archive().await;
    let expected_code = error.code();

    let result: Result<(), _> =
        with_writer_lease(&archive, LEASE, &CancellationToken::new(), async |_, _| {
            Err(error)
        })
        .await;

    assert_eq!(result.expect_err("failed operation").code(), expected_code);
    let reclaimed = archive
        .acquire_archive_lease(epoch(), LEASE)
        .await
        .expect("previous fence released");
    archive
        .release_archive_lease(&reclaimed, epoch())
        .await
        .expect("release test fence");
    close(directory, archive).await;
}

#[rstest]
#[case::success_survives_lost_release(Ok(()), None)]
#[case::operation_error_wins(Err(EngineError::InvalidClusterInput), Some("cluster_input_invalid"))]
#[tokio::test]
async fn already_released_fence_keeps_operation_result(
    #[case] outcome: Result<(), EngineError>,
    #[case] expected_code: Option<&str>,
) {
    let (directory, archive) = fixture_archive().await;

    let result = with_writer_lease(
        &archive,
        LEASE,
        &CancellationToken::new(),
        async |token, _| {
            let released = archive
                .release_archive_lease(token, epoch())
                .await
                .expect("release fixture fence before completion");
            assert!(released);
            outcome
        },
    )
    .await;

    assert_eq!(result.err().map(|error| error.code()), expected_code);
    close(directory, archive).await;
}

fn epoch() -> UtcTimestamp {
    UtcTimestamp::parse("1970-01-01T00:00:00Z").expect("epoch timestamp")
}

async fn fixture_archive() -> (PathBuf, Archive) {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    let directory =
        std::env::temp_dir().join(format!("forgesync-lease-{}-{sequence}", std::process::id()));
    std::fs::create_dir(&directory).expect("create fixture directory");
    let archive = Archive::create(directory.join("archive.sqlite"))
        .await
        .expect("create archive");
    (directory, archive)
}

async fn close(directory: PathBuf, archive: Archive) {
    archive.close().await;
    std::fs::remove_dir_all(directory).expect("remove fixture directory");
}
