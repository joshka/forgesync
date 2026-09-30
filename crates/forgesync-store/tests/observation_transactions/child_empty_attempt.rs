//! # Incomplete empty acquisition cannot erase a complete collection
//!
//! Two canonical comments are published first. A later generation finishes without any staged page
//! and declares incomplete pagination with zero received items. Lack of received data is not proof
//! that the provider's complete collection is empty, so both original members must survive.
//!
//! Membership and coverage are asserted separately: retained identities/payloads coexist with the
//! newer incomplete attempt's exact time, sequence, reason, zero count, and absent failure.
//! Complete empty replacement is exercised in the supersession sibling, not implied here.
//!
//! Reservation and publication are visible next to the public reads and expectations. Construction
//! fixtures perform no archive work, and on-disk resources are removed only after closure.

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::observation::{CollectionCompleteness, IncompleteReason, SourceClock};
use forgesync_store::archive::Archive;
use forgesync_store::families::{ChildFamilyObservation, ChildFamilyPage, ChildFamilyRequest};
use serde_json::json;

use crate::fixture::{
    discussion, incomplete, item, remove_archive, repository, temporary_archive_path, thread_id,
    thread_observation, timestamp,
};

#[tokio::test]
async fn incomplete_empty_collection_preserves_prior_membership() {
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
        .reserve_child_family_observation(ChildFamilyRequest {
            thread: &thread_id,
            family: EvidenceFamily::Comments,
            source_clock: &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            started_at: timestamp("2026-09-20T10:00:01Z"),
            request_scope: "GET /issues/1/comments",
        })
        .await
        .expect("reserve comments");
    assert!(comments.reserved);
    let original_members = vec![
        item("comment-1", json!({"body":"one"})),
        item("comment-2", json!({"body":"two"})),
    ];
    archive
        .stage_child_family_page(ChildFamilyPage {
            thread: &thread_id,
            family: EvidenceFamily::Comments,
            sequence: comments.sequence,
            page_index: 0,
            items: &original_members,
        })
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
    let members = archive
        .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("read comments");
    assert_eq!(members, original_members);

    let incomplete_empty = archive
        .reserve_child_family_observation(ChildFamilyRequest {
            thread: &thread_id,
            family: EvidenceFamily::Comments,
            source_clock: &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            started_at: timestamp("2026-09-20T10:00:04.5Z"),
            request_scope: "GET /issues/1/comments",
        })
        .await
        .expect("reserve incomplete empty comments");
    archive
        .finish_child_family_observation(ChildFamilyObservation {
            thread: &thread_id,
            family: EvidenceFamily::Comments,
            sequence: incomplete_empty.sequence,
            observed_at: timestamp("2026-09-20T10:00:04.6Z"),
            completeness: &incomplete(0),
            expected_pages: None,
            head_sha: None,
        })
        .await
        .expect("record incomplete empty collection");
    let members = archive
        .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("incomplete empty leaves prior membership intact");
    assert_eq!(members, original_members);

    let coverage = archive
        .family_coverage(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("read incomplete empty coverage");
    assert_eq!(
        coverage.state(),
        &CoverageState::Incomplete {
            observed_at: timestamp("2026-09-20T10:00:04.6Z"),
            sequence: incomplete_empty.sequence,
            reason: IncompleteReason::Pagination,
            received_items: 0,
            failure: None,
        }
    );

    archive.close().await;
    remove_archive(&path);
}
