//! # Atomic comment membership and coverage replacement
//!
//! A coverage-update trigger aborts replacement after membership work has begun.
//! Old membership and complete coverage survive together, and removing the trigger permits staged
//! retry. Archive creation, repository registration, reservation, and observation writes are
//! explicit. Construction fixtures provide checked identities and payloads without executing
//! transitions.
//!
//! The archive is on disk so committed state and transaction rollback are directly observable.
//! Fixed clocks separate provider time, acquisition time, and durable sequence without live timing.
//! Assertions describe this invariant rather than provider traversal or workflow scheduling.
//! Scenario cleanup follows closure of archive and raw inspection handles.

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::observation::{CollectionCompleteness, SourceClock};
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::families::ChildFamilyObservation;
use serde_json::json;

use crate::fixture::{
    discussion, item, remove_archive, repository, temporary_archive_path, thread_id,
    thread_observation, timestamp, writable_pool,
};

#[tokio::test]
async fn failed_membership_and_coverage_transaction_keeps_both_old_values() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository();
    archive
        .upsert_repository(&repository)
        .await
        .expect("register repository");
    let thread_id = thread_id(&repository.id);
    let thread_sequence = archive
        .reserve_observation_sequence(timestamp("2026-09-20T10:00:00Z"))
        .await
        .expect("reserve observation sequence");
    archive
        .apply_thread_observation(&thread_observation(
            discussion(&thread_id, "2026-09-20T10:00:00Z", "thread"),
            "2026-09-20T10:00:00Z",
            "2026-09-20T10:00:00Z",
            thread_sequence,
            CollectionCompleteness::Complete,
        ))
        .await
        .expect("apply parent");
    let first = archive
        .reserve_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:01Z"),
            "comments",
        )
        .await
        .expect("reserve first collection");
    archive
        .stage_child_family_page(
            &thread_id,
            EvidenceFamily::Comments,
            first.sequence,
            0,
            &[item("old", json!({"body":"old"}))],
        )
        .await
        .expect("stage first collection");
    archive
        .finish_child_family_observation(ChildFamilyObservation {
            thread: &thread_id,
            family: EvidenceFamily::Comments,
            sequence: first.sequence,
            observed_at: timestamp("2026-09-20T10:00:02Z"),
            completeness: &CollectionCompleteness::Complete,
            expected_pages: Some(1),
            head_sha: None,
        })
        .await
        .expect("commit first collection");

    let next = archive
        .reserve_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:03Z"),
            "comments",
        )
        .await
        .expect("reserve replacement collection");
    archive
        .stage_child_family_page(
            &thread_id,
            EvidenceFamily::Comments,
            next.sequence,
            0,
            &[item("new", json!({"body":"new"}))],
        )
        .await
        .expect("stage replacement");

    let trigger_pool = writable_pool(&path).await;
    sqlx::query(
        "CREATE TRIGGER reject_comment_coverage BEFORE UPDATE ON family_coverage WHEN NEW.family = 'comments' BEGIN SELECT RAISE(ABORT, 'forced coverage failure'); END",
    )
    .execute(&trigger_pool)
    .await
    .expect("install test trigger");
    let observation = ChildFamilyObservation {
        thread: &thread_id,
        family: EvidenceFamily::Comments,
        sequence: next.sequence,
        observed_at: timestamp("2026-09-20T10:00:04Z"),
        completeness: &CollectionCompleteness::Complete,
        expected_pages: Some(1),
        head_sha: None,
    };
    let error = archive
        .finish_child_family_observation(observation)
        .await
        .expect_err("coverage trigger aborts finalization");
    assert!(matches!(&error, StoreError::Database(_)));
    assert!(error.to_string().contains("forced coverage failure"));

    let members = archive
        .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("old membership survives rollback");
    assert_eq!(members, vec![item("old", json!({"body":"old"}))]);
    let coverage = archive
        .family_coverage(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("old coverage survives rollback");
    assert!(matches!(
        coverage.state(),
        CoverageState::Complete { sequence, item_count: 1, .. } if *sequence == first.sequence
    ));

    sqlx::query("DROP TRIGGER reject_comment_coverage")
        .execute(&trigger_pool)
        .await
        .expect("remove test trigger");
    trigger_pool.close().await;
    archive
        .finish_child_family_observation(ChildFamilyObservation {
            thread: &thread_id,
            family: EvidenceFamily::Comments,
            sequence: next.sequence,
            observed_at: timestamp("2026-09-20T10:00:04Z"),
            completeness: &CollectionCompleteness::Complete,
            expected_pages: Some(1),
            head_sha: None,
        })
        .await
        .expect("retry the still-staged generation");
    assert_eq!(
        archive
            .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
            .await
            .expect("read replacement membership"),
        vec![item("new", json!({"body":"new"}))]
    );

    archive.close().await;
    remove_archive(&path);
}
