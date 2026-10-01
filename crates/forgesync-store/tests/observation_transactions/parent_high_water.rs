//! # Parent source and acquisition high waters
//!
//! A delayed observation of the same source revision cannot replace content acquired later, and a
//! newer source revision replaces content regardless of its acquisition order.

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_store::archive::Archive;
use forgesync_store::observations::ObservationDisposition;

use crate::fixture::{
    discussion, remove_archive, repository, temporary_archive_path, thread_id, thread_observation,
    timestamp,
};

#[tokio::test]
async fn delayed_parent_observations_do_not_replace_later_acquisition() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository();
    archive
        .upsert_repository(&repository)
        .await
        .expect("register repository");
    let thread_id = thread_id(&repository.id);
    let mut sequences = Vec::new();
    for second in 0..3 {
        let at = timestamp(&format!("2026-09-20T10:00:0{second}Z"));
        sequences.push(
            archive
                .reserve_observation_sequence(at)
                .await
                .expect("reserve observation sequence"),
        );
    }
    let [first_sequence, second_sequence, third_sequence] = sequences[..] else {
        unreachable!("three sequences were reserved");
    };

    let current = discussion(&thread_id, "2026-09-20T10:00:00Z", "current");
    let applied = archive
        .apply_thread_observation(
            &thread_observation(current.clone(), "2026-09-20T10:00:01Z", second_sequence),
            None,
        )
        .await
        .expect("apply later acquisition");
    assert_eq!(applied.disposition, ObservationDisposition::Applied);
    assert_eq!(applied.evidence_sequence, Some(second_sequence));

    let delayed = discussion(&thread_id, "2026-09-20T10:00:00Z", "delayed conflicting");
    let skipped = archive
        .apply_thread_observation(
            &thread_observation(delayed, "2026-09-20T10:00:02Z", first_sequence),
            None,
        )
        .await
        .expect("skip delayed conflicting observation");
    assert_eq!(skipped.disposition, ObservationDisposition::Skipped);
    assert_eq!(skipped.high_water_sequence, second_sequence);

    let replayed = archive
        .apply_thread_observation(
            &thread_observation(current, "2026-09-20T10:00:02Z", first_sequence),
            None,
        )
        .await
        .expect("replay delayed identical observation");
    assert_eq!(replayed.disposition, ObservationDisposition::Replayed);
    assert_eq!(replayed.evidence_sequence, Some(second_sequence));

    let newer = discussion(&thread_id, "2026-09-20T10:00:05Z", "newer source");
    let newer_applied = archive
        .apply_thread_observation(
            &thread_observation(newer, "2026-09-20T10:00:03Z", third_sequence),
            None,
        )
        .await
        .expect("apply newer source revision");
    assert_eq!(newer_applied.disposition, ObservationDisposition::Applied);
    assert_eq!(newer_applied.high_water_sequence, third_sequence);

    let coverage = archive
        .family_coverage(&thread_id, EvidenceFamily::Threads)
        .await
        .expect("read thread coverage");
    assert_eq!(
        coverage.state(),
        &CoverageState::Complete {
            observed_at: timestamp("2026-09-20T10:00:03Z"),
            sequence: third_sequence,
            item_count: 1,
        }
    );
    archive.close().await;
    remove_archive(&path);
}
