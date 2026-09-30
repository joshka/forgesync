//! # Partial publication records new coverage without replacing membership
//!
//! A complete two-comment baseline is followed by a new generation with one received replacement.
//! Finalization reports that one received item, but incomplete pagination cannot authorize
//! replacing either prior canonical member. The local read compares both original identities and
//! payloads.
//!
//! Coverage describes the newer incomplete attempt independently of retained complete membership:
//! its acquisition time, sequence, reason, received count, and absence of failure are checked
//! exactly. Source and acquisition clocks are explicit rather than inferred from stored row counts.
//!
//! All archive operations remain in this linear before/after case. Fixtures only build values;
//! provider traversal and engine retry selection belong to other integration suites. Cleanup
//! follows explicit archive closure.

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
async fn partial_collection_preserves_prior_complete_membership() {
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

    let partial = archive
        .reserve_child_family_observation(ChildFamilyRequest {
            thread: &thread_id,
            family: EvidenceFamily::Comments,
            source_clock: &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            started_at: timestamp("2026-09-20T10:00:03Z"),
            request_scope: "GET /issues/1/comments",
        })
        .await
        .expect("reserve partial comments");
    assert!(partial.reserved);
    archive
        .stage_child_family_page(ChildFamilyPage {
            thread: &thread_id,
            family: EvidenceFamily::Comments,
            sequence: partial.sequence,
            page_index: 0,
            items: &[item("comment-3", json!({"body":"partial"}))],
        })
        .await
        .expect("stage partial page");
    let partial_result = archive
        .finish_child_family_observation(ChildFamilyObservation {
            thread: &thread_id,
            family: EvidenceFamily::Comments,
            sequence: partial.sequence,
            observed_at: timestamp("2026-09-20T10:00:04Z"),
            completeness: &incomplete(1),
            expected_pages: None,
            head_sha: None,
        })
        .await
        .expect("record incomplete collection");
    assert_eq!(partial_result.item_count, 1);
    let members = archive
        .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("previous complete membership remains");
    assert_eq!(members, original_members);
    let coverage = archive
        .family_coverage(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("read incomplete coverage");
    assert_eq!(
        coverage.state(),
        &CoverageState::Incomplete {
            observed_at: timestamp("2026-09-20T10:00:04Z"),
            sequence: partial.sequence,
            reason: IncompleteReason::Pagination,
            received_items: 1,
            failure: None,
        }
    );

    archive.close().await;
    remove_archive(&path);
}
