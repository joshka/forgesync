//! # Only the current empty generation can erase canonical comments
//!
//! A complete two-member collection establishes the baseline. Two later generations reserve the
//! same source clock; the second reservation supersedes the first through acquisition ordering.
//! Completing the older generation with zero pages must be skipped and retain both old members.
//!
//! The newer generation then completes empty and is allowed to replace membership. The resulting
//! coverage names its exact acquisition time and sequence with zero complete items, distinguishing
//! legitimate emptiness from missing or incomplete evidence.
//!
//! These dependent transitions remain together because the current-versus-superseded distinction
//! requires both reservations. Every archive operation and read is explicit; fixtures construct
//! only source values and staged members. Closure precedes database cleanup.

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::observation::{CollectionCompleteness, SourceClock};
use forgesync_store::archive::Archive;
use forgesync_store::families::{ChildFamilyObservation, ChildFamilyPage, ChildFamilyRequest};
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
                thread_sequence,
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
                source_clock: &SourceClock::Valid(timestamp("2026-09-20T10:00:00Z")),
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

    let stale_comments = archive
        .reserve_child_family_observation_fenced(
            ChildFamilyRequest {
                thread: &thread_id,
                family: EvidenceFamily::Comments,
                source_clock: &SourceClock::Valid(timestamp("2026-09-20T10:00:00Z")),
                started_at: timestamp("2026-09-20T10:00:07Z"),
                request_scope: "GET /issues/1/comments",
            },
            &lease,
        )
        .await
        .expect("reserve stale comments generation");
    let empty_comments = archive
        .reserve_child_family_observation_fenced(
            ChildFamilyRequest {
                thread: &thread_id,
                family: EvidenceFamily::Comments,
                source_clock: &SourceClock::Valid(timestamp("2026-09-20T10:00:00Z")),
                started_at: timestamp("2026-09-20T10:00:07.1Z"),
                request_scope: "GET /issues/1/comments",
            },
            &lease,
        )
        .await
        .expect("reserve current empty comments");
    let stale_result = archive
        .finish_child_family_observation_fenced(
            ChildFamilyObservation {
                thread: &thread_id,
                family: EvidenceFamily::Comments,
                sequence: stale_comments.sequence,
                observed_at: timestamp("2026-09-20T10:00:07.2Z"),
                completeness: &CollectionCompleteness::Complete,
                expected_pages: Some(0),
                head_sha: None,
            },
            &lease,
        )
        .await
        .expect("superseded generation is skipped");
    assert_eq!(stale_result.disposition, ObservationDisposition::Skipped);
    let members = archive
        .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("stale empty generation leaves membership intact");
    assert_eq!(members, original_members);
    archive
        .finish_child_family_observation_fenced(
            ChildFamilyObservation {
                thread: &thread_id,
                family: EvidenceFamily::Comments,
                sequence: empty_comments.sequence,
                observed_at: timestamp("2026-09-20T10:00:08Z"),
                completeness: &CollectionCompleteness::Complete,
                expected_pages: Some(0),
                head_sha: None,
            },
            &lease,
        )
        .await
        .expect("replace with complete empty collection");
    let members = archive
        .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("empty comments");
    assert!(members.is_empty());
    let coverage = archive
        .family_coverage(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("read complete empty coverage");
    assert_eq!(
        coverage.state(),
        &CoverageState::Complete {
            observed_at: timestamp("2026-09-20T10:00:08Z"),
            sequence: empty_comments.sequence,
            item_count: 0,
        }
    );

    archive.close().await;
    remove_archive(&path);
}
