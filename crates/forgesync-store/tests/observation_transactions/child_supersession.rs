//! # Superseded empty generation cannot replace membership
//!
//! This case reserves, stages, and finishes the selected comment generations directly.
//! A complete two-member baseline establishes the canonical state before replacement attempts.
//! Payload construction supplies exact identities and JSON without performing archive operations.
//! Source time, acquisition time, and sequence remain explicit and independently controlled.
//!
//! The archive is on disk and each transition is visible in execution order.
//! Assertions compare retained membership and the relevant publication outcome or coverage.
//! Provider pagination, lease scheduling, and engine retries are separate integration concerns.
//! The scenario closes its archive before removing the database and fixed sidecars.

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::observation::{CollectionCompleteness, SourceClock};
use forgesync_store::archive::Archive;
use forgesync_store::families::ChildFamilyObservation;
use forgesync_store::observations::ObservationDisposition;
use serde_json::json;

use crate::fixture::{
    discussion, item, remove_archive, repository, temporary_archive_path, thread_id,
    thread_observation, timestamp,
};

#[tokio::test]
async fn superseded_empty_generation_cannot_replace_membership() {
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
        .expect("apply parent thread");

    let comments = archive
        .reserve_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:01Z"),
            "GET /issues/1/comments",
        )
        .await
        .expect("reserve comments");
    assert!(comments.reserved);
    let original_members = vec![
        item("comment-1", json!({"body":"one"})),
        item("comment-2", json!({"body":"two"})),
    ];
    archive
        .stage_child_family_page(
            &thread_id,
            EvidenceFamily::Comments,
            comments.sequence,
            0,
            &original_members,
        )
        .await
        .expect("stage comments page");
    let complete = archive
        .finish_child_family_observation(ChildFamilyObservation {
            thread: &thread_id,
            family: EvidenceFamily::Comments,
            sequence: comments.sequence,
            observed_at: timestamp("2026-09-20T10:00:02Z"),
            completeness: &CollectionCompleteness::Complete,
            expected_pages: Some(1),
            head_sha: None,
        })
        .await
        .expect("complete comments");
    assert_eq!(complete.item_count, 2);
    assert_eq!(
        archive
            .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
            .await
            .expect("read comments"),
        original_members
    );

    let stale_comments = archive
        .reserve_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:07Z"),
            "GET /issues/1/comments",
        )
        .await
        .expect("reserve stale comments generation");
    let empty_comments = archive
        .reserve_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:07.1Z"),
            "GET /issues/1/comments",
        )
        .await
        .expect("reserve current empty comments");
    let stale_result = archive
        .finish_child_family_observation(ChildFamilyObservation {
            thread: &thread_id,
            family: EvidenceFamily::Comments,
            sequence: stale_comments.sequence,
            observed_at: timestamp("2026-09-20T10:00:07.2Z"),
            completeness: &CollectionCompleteness::Complete,
            expected_pages: Some(0),
            head_sha: None,
        })
        .await
        .expect("superseded generation is skipped");
    assert_eq!(stale_result.disposition, ObservationDisposition::Skipped);
    assert_eq!(
        archive
            .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
            .await
            .expect("stale empty generation leaves membership intact"),
        original_members
    );
    archive
        .finish_child_family_observation(ChildFamilyObservation {
            thread: &thread_id,
            family: EvidenceFamily::Comments,
            sequence: empty_comments.sequence,
            observed_at: timestamp("2026-09-20T10:00:08Z"),
            completeness: &CollectionCompleteness::Complete,
            expected_pages: Some(0),
            head_sha: None,
        })
        .await
        .expect("replace with complete empty collection");
    assert!(
        archive
            .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
            .await
            .expect("empty comments")
            .is_empty()
    );
    let coverage = archive
        .family_coverage(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("read complete empty coverage");
    assert!(matches!(
        coverage.state(),
        CoverageState::Complete { item_count: 0, .. }
    ));

    archive.close().await;
    remove_archive(&path);
}
