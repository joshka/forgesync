//! # Parent replay and conflicting payloads
//!
//! Rejection compares the specific error and checks that the canonical discussion is unchanged.

use forgesync_core::identity::ThreadReference;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::observations::ObservationDisposition;

use crate::fixture::{
    discussion, remove_archive, repository, temporary_archive_path, thread_id, thread_observation,
    timestamp,
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
        sequence,
    );

    archive
        .apply_thread_observation(&observation, None)
        .await
        .expect("apply first observation");
    let replay = archive
        .apply_thread_observation(&observation, None)
        .await
        .expect("replay same observation");
    assert_eq!(replay.disposition, ObservationDisposition::Replayed);

    let reference = ThreadReference::new(repository.id.clone(), thread_id.number());
    let current = archive
        .thread_detail(&reference)
        .await
        .expect("read retained discussion");
    assert_eq!(&current.summary.discussion, &observation.discussion);
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
        sequence,
    );

    archive
        .apply_thread_observation(&observation, None)
        .await
        .expect("apply first observation");
    let conflicting = thread_observation(
        discussion(&thread_id, "2026-09-20T10:00:00Z", "conflict"),
        "2026-09-20T10:00:00Z",
        sequence,
    );
    let rejected = archive.apply_thread_observation(&conflicting, None).await;
    assert!(matches!(rejected, Err(StoreError::ConflictingObservation)));

    let reference = ThreadReference::new(repository.id.clone(), thread_id.number());
    let current = archive
        .thread_detail(&reference)
        .await
        .expect("read retained discussion");
    assert_eq!(&current.summary.discussion, &observation.discussion);
    archive.close().await;
    remove_archive(&path);
}
