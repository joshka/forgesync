//! # Atomic review-thread membership and head context
//!
//! A coverage-update trigger aborts a proposed review-thread snapshot.
//! Canonical membership and eligibility for the original head must survive that failed transaction.
//! Archive creation, repository registration, reservation, and observation writes are explicit.
//! Construction fixtures provide checked identities and payloads without executing transitions.
//!
//! The archive is on disk so committed state and transaction rollback are directly observable.
//! Fixed clocks separate provider time, acquisition time, and durable sequence without live timing.
//! Assertions describe this invariant rather than provider traversal or workflow scheduling.
//! Scenario cleanup follows closure of archive and raw inspection handles.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::CommitSha;
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
async fn failed_review_thread_snapshot_rolls_back_membership_coverage_and_head_context() {
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
