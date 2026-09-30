//! # Parent replay and conflicting source evidence
//!
//! Independent cases cover exact replay, tied payload conflict, and incompatible malformed clocks.
//! Rejection compares the specific error and checks that canonical title remains unchanged.
//! Archive creation, repository registration, reservation, and observation writes are explicit.
//! Construction fixtures provide checked identities and payloads without executing transitions.
//!
//! The archive is on disk so committed state and transaction rollback are directly observable.
//! Fixed clocks separate provider time, acquisition time, and durable sequence without live timing.
//! Assertions describe this invariant rather than provider traversal or workflow scheduling.
//! Scenario cleanup follows closure of archive and raw inspection handles.

use forgesync_core::observation::CollectionCompleteness;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::observations::ObservationDisposition;

use crate::fixture::{
    discussion, read_current_thread_title, remove_archive, repository, temporary_archive_path,
    thread_id, thread_observation, timestamp,
};

#[tokio::test]
async fn identical_observation_replay_preserves_canonical_content() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository();
    archive
        .upsert_repository(&repository)
        .await
        .expect("register repository");
    let thread_id = thread_id(&repository.id);
    let sequence = archive
        .reserve_observation_sequence(timestamp("2026-09-20T10:00:00Z"))
        .await
        .expect("reserve observation sequence");
    let observation = thread_observation(
        discussion(&thread_id, "2026-09-20T10:00:00Z", "same"),
        "2026-09-20T10:00:00Z",
        "2026-09-20T10:00:00Z",
        sequence,
        CollectionCompleteness::Complete,
    );

    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply first observation");
    let replay = archive
        .apply_thread_observation(&observation)
        .await
        .expect("replay same observation");
    assert_eq!(replay.disposition, ObservationDisposition::Replayed);

    let current = read_current_thread_title(&path).await;
    assert_eq!(current, "same");
    archive.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn tied_conflicting_payload_is_rejected_without_replacing_content() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository();
    archive
        .upsert_repository(&repository)
        .await
        .expect("register repository");
    let thread_id = thread_id(&repository.id);
    let sequence = archive
        .reserve_observation_sequence(timestamp("2026-09-20T10:00:00Z"))
        .await
        .expect("reserve observation sequence");
    let observation = thread_observation(
        discussion(&thread_id, "2026-09-20T10:00:00Z", "same"),
        "2026-09-20T10:00:00Z",
        "2026-09-20T10:00:00Z",
        sequence,
        CollectionCompleteness::Complete,
    );

    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply first observation");
    let conflicting = thread_observation(
        discussion(&thread_id, "2026-09-20T10:00:00Z", "conflict"),
        "2026-09-20T10:00:00Z",
        "2026-09-20T10:00:00Z",
        sequence,
        CollectionCompleteness::Complete,
    );
    assert!(matches!(
        archive.apply_thread_observation(&conflicting).await,
        Err(StoreError::ConflictingObservation)
    ));

    let current = read_current_thread_title(&path).await;
    assert_eq!(current, "same");
    archive.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn different_malformed_source_clocks_are_rejected_without_replacing_content() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository();
    archive
        .upsert_repository(&repository)
        .await
        .expect("register repository");
    let thread_id = thread_id(&repository.id);
    let first_sequence = archive
        .reserve_observation_sequence(timestamp("2026-09-20T10:00:00Z"))
        .await
        .expect("reserve observation sequence");
    let second_sequence = archive
        .reserve_observation_sequence(timestamp("2026-09-20T10:00:01Z"))
        .await
        .expect("reserve observation sequence");
    let first = thread_observation(
        discussion(&thread_id, "2026-09-20T10:00:00Z", "first"),
        "not-a-time-a",
        "2026-09-20T10:00:00Z",
        first_sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&first)
        .await
        .expect("apply malformed source clock");
    let second = thread_observation(
        discussion(&thread_id, "2026-09-20T10:00:00Z", "second"),
        "not-a-time-b",
        "2026-09-20T10:00:01Z",
        second_sequence,
        CollectionCompleteness::Complete,
    );
    assert!(matches!(
        archive.apply_thread_observation(&second).await,
        Err(StoreError::AmbiguousObservationClocks { .. })
    ));

    let current = read_current_thread_title(&path).await;
    assert_eq!(current, "first");
    archive.close().await;
    remove_archive(&path);
}
