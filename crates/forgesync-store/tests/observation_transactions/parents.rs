//! # Parent observation cases
//!
//! These cases apply normalized discussion snapshots and inspect the resulting canonical thread
//! state. They protect identity mapping and transactional application. Child resources are
//! independent families; a parent write should not silently claim their coverage.
//!
//! Replay, tied payload conflicts, and malformed source clocks have independent scenarios.
//! Every scenario creates its archive, registers the repository, and reserves sequences directly.
//! Pure fixtures supply payloads and clocks; raw inspection verifies retained canonical title.
//! Arithmetic precedence is tested separately in `ordering` without an archive.
//! The high-water scenario retains its linear hydration sequence because each step constrains the
//! next: source selection and complete-evidence selection advance independently.

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::observation::CollectionCompleteness;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::observations::ObservationDisposition;

use crate::fixture::{
    discussion, incomplete, read_current_thread_title, remove_archive, repository,
    temporary_archive_path, thread_id, thread_observation, timestamp,
};

#[tokio::test]
async fn parent_observations_keep_separate_source_and_evidence_high_waters() {
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
    let third_sequence = archive
        .reserve_observation_sequence(timestamp("2026-09-20T10:00:02Z"))
        .await
        .expect("reserve observation sequence");
    let fourth_sequence = archive
        .reserve_observation_sequence(timestamp("2026-09-20T10:00:03Z"))
        .await
        .expect("reserve observation sequence");

    let first = discussion(&thread_id, "2026-09-20T10:00:00Z", "first");
    let applied = archive
        .apply_thread_observation(&thread_observation(
            first.clone(),
            "2026-09-20T10:00:00Z",
            "2026-09-20T10:00:00Z",
            first_sequence,
            CollectionCompleteness::Complete,
        ))
        .await
        .expect("apply initial complete observation");
    assert_eq!(applied.disposition, ObservationDisposition::Applied);
    assert_eq!(applied.high_water_sequence, first_sequence);
    assert_eq!(applied.evidence_sequence, Some(first_sequence));

    let replayed_incomplete = archive
        .apply_thread_observation(&thread_observation(
            first.clone(),
            "2026-09-20T10:00:00Z",
            "2026-09-20T10:00:01Z",
            third_sequence,
            incomplete(1),
        ))
        .await
        .expect("apply same-payload incomplete generation");
    assert_eq!(
        replayed_incomplete.disposition,
        ObservationDisposition::Applied
    );
    assert_eq!(replayed_incomplete.high_water_sequence, third_sequence);
    assert_eq!(replayed_incomplete.evidence_sequence, Some(first_sequence));

    let newer_parent = discussion(&thread_id, "2026-09-20T10:00:01Z", "newer");
    let incomplete_newer = archive
        .apply_thread_observation(&thread_observation(
            newer_parent.clone(),
            "2026-09-20T10:00:01Z",
            "2026-09-20T10:00:02Z",
            fourth_sequence,
            incomplete(1),
        ))
        .await
        .expect("apply newer incomplete parent");
    assert_eq!(incomplete_newer.high_water_sequence, fourth_sequence);
    assert_eq!(incomplete_newer.evidence_sequence, Some(first_sequence));

    let hydrated = archive
        .apply_thread_observation(&thread_observation(
            newer_parent.clone(),
            "2026-09-20T10:00:01Z",
            "2026-09-20T10:00:03Z",
            second_sequence,
            CollectionCompleteness::Complete,
        ))
        .await
        .expect("hydrate same source below the parent high-water mark");
    assert_eq!(hydrated.disposition, ObservationDisposition::Applied);
    assert_eq!(hydrated.high_water_sequence, fourth_sequence);
    assert_eq!(hydrated.evidence_sequence, Some(second_sequence));

    let delayed = discussion(
        &thread_id,
        "2026-09-20T10:00:01Z",
        "delayed conflicting payload",
    );
    let skipped = archive
        .apply_thread_observation(&thread_observation(
            delayed,
            "2026-09-20T10:00:01Z",
            "2026-09-20T10:00:04Z",
            third_sequence,
            CollectionCompleteness::Complete,
        ))
        .await
        .expect("skip delayed conflicting hydration");
    assert_eq!(skipped.disposition, ObservationDisposition::Skipped);
    assert_eq!(skipped.high_water_sequence, fourth_sequence);
    assert_eq!(skipped.evidence_sequence, Some(second_sequence));

    let coverage = archive
        .family_coverage(&thread_id, EvidenceFamily::Threads)
        .await
        .expect("read thread coverage");
    assert!(matches!(
        coverage.state(),
        CoverageState::Complete { sequence, .. } if *sequence == second_sequence
    ));
    archive.close().await;
    remove_archive(&path);
}

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
