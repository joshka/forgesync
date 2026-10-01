//! # Staged-page replay does not duplicate canonical membership
//!
//! One reserved comment generation receives the identical page twice before it is finalized.
//! The repeated page has the same provider identities and payloads, so complete publication must
//! retain exactly the two supplied members rather than append duplicate rows.
//!
//! The page replay occurs before the first canonical collection exists. This case therefore checks
//! staging idempotence and final membership, not preservation across two completed generations.
//! Reservation, both stage writes, finalization, and the membership read remain explicit.
//!
//! Pure fixtures construct parent evidence and JSON members; they perform no acquisition or writes.
//! Partial collections and superseded generations have sibling scenarios. Archive closure precedes
//! removal of the on-disk database and its sidecars.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::observation::{CollectionCompleteness, SourceClock};
use forgesync_store::archive::Archive;
use forgesync_store::families::{ChildFamilyObservation, ChildFamilyPage, ChildFamilyRequest};
use serde_json::json;

use crate::fixture::{
    discussion, item, remove_archive, repository, temporary_archive_path, thread_id,
    thread_observation, timestamp,
};

#[tokio::test]
async fn replaying_a_staged_page_preserves_complete_membership() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let lease = crate::common::lease(&archive).await;
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
        .apply_thread_observation(
            &thread_observation(
                discussion(&thread_id, "2026-09-20T10:00:00Z", "thread"),
                "2026-09-20T10:00:00Z",
                "2026-09-20T10:00:00Z",
                thread_sequence,
                CollectionCompleteness::Complete,
            ),
            None,
        )
        .await
        .expect("apply parent thread");

    let comments = archive
        .reserve_child_family_observation_fenced(
            ChildFamilyRequest {
                thread: &thread_id,
                family: EvidenceFamily::Comments,
                source_clock: &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
                started_at: timestamp("2026-09-20T10:00:01Z"),
                request_scope: "GET /issues/1/comments",
            },
            &lease,
        )
        .await
        .expect("reserve comments");
    assert!(comments.reserved);
    let original_members = vec![
        item("comment-1", json!({"body":"one"})),
        item("comment-2", json!({"body":"two"})),
    ];
    archive
        .stage_child_family_page_fenced(
            ChildFamilyPage {
                thread: &thread_id,
                family: EvidenceFamily::Comments,
                sequence: comments.sequence,
                page_index: 0,
                items: &original_members,
            },
            &lease,
        )
        .await
        .expect("stage comments page");
    archive
        .stage_child_family_page_fenced(
            ChildFamilyPage {
                thread: &thread_id,
                family: EvidenceFamily::Comments,
                sequence: comments.sequence,
                page_index: 0,
                items: &original_members,
            },
            &lease,
        )
        .await
        .expect("replay same page");
    let complete = archive
        .finish_child_family_observation_fenced(
            ChildFamilyObservation {
                thread: &thread_id,
                family: EvidenceFamily::Comments,
                sequence: comments.sequence,
                observed_at: timestamp("2026-09-20T10:00:02Z"),
                completeness: &CollectionCompleteness::Complete,
                expected_pages: Some(1),
                head_sha: None,
            },
            &lease,
        )
        .await
        .expect("complete comments");
    assert_eq!(complete.item_count, 2);
    let members = archive
        .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("read comments");
    assert_eq!(members, original_members);

    archive.close().await;
    remove_archive(&path);
}
