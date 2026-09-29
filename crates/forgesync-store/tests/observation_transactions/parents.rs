//! # Parent observation cases
//!
//! These cases apply normalized discussion snapshots and inspect the resulting canonical thread
//! state. They protect identity mapping and transactional application. Child resources are
//! independent families; a parent write should not silently claim their coverage.

use super::{
    CollectionCompleteness, CoverageState, EvidenceFamily, ObservationDisposition, Ordering,
    SourceClock, StoreError, compare_revision_observation_order, create_archive_with_repository,
    discussion, incomplete, read_current_thread_title, remove_archive, reserve,
    temporary_archive_path, thread_observation,
};

#[tokio::test]
async fn parent_observations_keep_separate_source_and_evidence_high_waters() {
    let path = temporary_archive_path();
    let (archive, repository_id, thread_id) = create_archive_with_repository(&path).await;
    let first_sequence = reserve(&archive, "2026-09-20T10:00:00Z").await;
    let second_sequence = reserve(&archive, "2026-09-20T10:00:01Z").await;
    let third_sequence = reserve(&archive, "2026-09-20T10:00:02Z").await;
    let fourth_sequence = reserve(&archive, "2026-09-20T10:00:03Z").await;

    let first = discussion(&repository_id, &thread_id, "2026-09-20T10:00:00Z", "first");
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

    let newer_parent = discussion(&repository_id, &thread_id, "2026-09-20T10:00:01Z", "newer");
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
        &repository_id,
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
async fn tied_conflicts_are_rejected_and_identical_observations_are_idempotent() {
    let path = temporary_archive_path();
    let (archive, repository_id, thread_id) = create_archive_with_repository(&path).await;
    let sequence = reserve(&archive, "2026-09-20T10:00:00Z").await;
    let observation = thread_observation(
        discussion(&repository_id, &thread_id, "2026-09-20T10:00:00Z", "same"),
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

    let conflicting = thread_observation(
        discussion(
            &repository_id,
            &thread_id,
            "2026-09-20T10:00:00Z",
            "conflict",
        ),
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
async fn malformed_source_clocks_are_ambiguous_but_revision_sequences_remain_distinct() {
    let path = temporary_archive_path();
    let (archive, repository_id, thread_id) = create_archive_with_repository(&path).await;
    let first_sequence = reserve(&archive, "2026-09-20T10:00:00Z").await;
    let second_sequence = reserve(&archive, "2026-09-20T10:00:01Z").await;
    let first = thread_observation(
        discussion(&repository_id, &thread_id, "2026-09-20T10:00:00Z", "first"),
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
        discussion(&repository_id, &thread_id, "2026-09-20T10:00:00Z", "second"),
        "not-a-time-b",
        "2026-09-20T10:00:01Z",
        second_sequence,
        CollectionCompleteness::Complete,
    );
    assert!(matches!(
        archive.apply_thread_observation(&second).await,
        Err(StoreError::AmbiguousObservationClocks { .. })
    ));

    let revision_order = compare_revision_observation_order(
        &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
        Some(second_sequence),
        &SourceClock::from_raw(Some("2026-09-20T10:00:01Z")),
        Some(first_sequence),
    )
    .expect("revision evidence compares acquisition sequences first");
    assert_eq!(revision_order, Ordering::Greater);
    archive.close().await;
    remove_archive(&path);
}
