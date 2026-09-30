//! # Transaction rollback cases
//!
//! These cases force failures during observation application and inspect the archive afterward.
//! They establish which writes are atomic and which prior evidence remains. A future store change
//! should leave a failed transaction visible as failure, not as partially canonical content.

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::CommitSha;
use forgesync_core::observation::{CollectionCompleteness, SourceClock};
use forgesync_store::error::StoreError;
use forgesync_store::families::ChildFamilyObservation;
use serde_json::json;

use crate::fixture::{
    create_archive_with_repository, discussion, item, remove_archive, reserve,
    temporary_archive_path, thread_observation, timestamp, writable_pool,
};

#[tokio::test]
async fn failed_membership_and_coverage_transaction_keeps_both_old_values() {
    let path = temporary_archive_path();
    let (archive, thread_id) = create_archive_with_repository(&path).await;
    let thread_sequence = reserve(&archive, "2026-09-20T10:00:00Z").await;
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

#[tokio::test]
async fn failed_review_thread_snapshot_rolls_back_membership_coverage_and_head_context() {
    let path = temporary_archive_path();
    let (archive, thread_id) = create_archive_with_repository(&path).await;
    let thread_sequence = reserve(&archive, "2026-09-20T10:00:00Z").await;
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

    let head =
        CommitSha::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect("review-thread head SHA");
    let first = archive
        .reserve_child_family_observation(
            &thread_id,
            EvidenceFamily::ReviewThreads,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:01Z"),
            "review threads",
        )
        .await
        .expect("reserve first review-thread snapshot");
    archive
        .stage_child_family_page(
            &thread_id,
            EvidenceFamily::ReviewThreads,
            first.sequence,
            0,
            &[item("old", json!({"resolution":"open"}))],
        )
        .await
        .expect("stage initial review thread");
    archive
        .finish_child_family_observation(ChildFamilyObservation {
            thread: &thread_id,
            family: EvidenceFamily::ReviewThreads,
            sequence: first.sequence,
            observed_at: timestamp("2026-09-20T10:00:02Z"),
            completeness: &CollectionCompleteness::Complete,
            expected_pages: Some(1),
            head_sha: Some(&head),
        })
        .await
        .expect("commit initial review-thread snapshot");

    let next = archive
        .reserve_child_family_observation(
            &thread_id,
            EvidenceFamily::ReviewThreads,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:03Z"),
            "review threads",
        )
        .await
        .expect("reserve replacement review-thread snapshot");
    archive
        .stage_child_family_page(
            &thread_id,
            EvidenceFamily::ReviewThreads,
            next.sequence,
            0,
            &[item("new", json!({"resolution":"resolved"}))],
        )
        .await
        .expect("stage replacement review thread");

    let trigger_pool = writable_pool(&path).await;
    sqlx::query(
        "CREATE TRIGGER reject_review_thread_coverage BEFORE UPDATE ON family_coverage WHEN NEW.family = 'review_threads' BEGIN SELECT RAISE(ABORT, 'forced coverage failure'); END",
    )
    .execute(&trigger_pool)
    .await
    .expect("install test trigger");
    let error = archive
        .finish_child_family_observation(ChildFamilyObservation {
            thread: &thread_id,
            family: EvidenceFamily::ReviewThreads,
            sequence: next.sequence,
            observed_at: timestamp("2026-09-20T10:00:04Z"),
            completeness: &CollectionCompleteness::Complete,
            expected_pages: Some(1),
            head_sha: Some(&head),
        })
        .await
        .expect_err("review-thread coverage trigger aborts finalization");
    assert!(matches!(&error, StoreError::Database(_)));
    assert!(error.to_string().contains("forced coverage failure"));

    assert_eq!(
        archive
            .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::ReviewThreads)
            .await
            .expect("old membership remains after failed transaction"),
        vec![item("old", json!({"resolution":"open"}))]
    );
    assert!(
        archive
            .pull_request_family_is_current_for_head(
                &thread_id,
                EvidenceFamily::ReviewThreads,
                &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
                &head,
            )
            .await
            .expect("old complete coverage and head context remain")
    );

    trigger_pool.close().await;
    archive.close().await;
    remove_archive(&path);
}
