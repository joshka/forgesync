//! # Child-family transaction cases
//!
//! These cases reserve, stage, and finish independently paginated evidence. They protect the rule
//! that incomplete collection cannot replace prior complete membership. Coverage records the
//! attempt even when its staged pages are not canonical.

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::CommitSha;
use forgesync_core::observation::{CollectionCompleteness, IncompleteReason, SourceClock};
use forgesync_store::archive::Archive;
use forgesync_store::families::ChildFamilyObservation;
use forgesync_store::observations::ObservationDisposition;
use serde_json::json;

use crate::fixture::{
    discussion, incomplete, item, remove_archive, repository, temporary_archive_path, thread_id,
    thread_observation, timestamp,
};

#[tokio::test]
async fn child_families_stage_pages_and_only_complete_results_replace_membership() {
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
    archive
        .stage_child_family_page(
            &thread_id,
            EvidenceFamily::Comments,
            comments.sequence,
            0,
            &original_members,
        )
        .await
        .expect("replay same page");
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

    let partial = archive
        .reserve_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:03Z"),
            "GET /issues/1/comments",
        )
        .await
        .expect("reserve partial comments");
    assert!(partial.reserved);
    archive
        .stage_child_family_page(
            &thread_id,
            EvidenceFamily::Comments,
            partial.sequence,
            0,
            &[item("comment-3", json!({"body":"partial"}))],
        )
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
    assert_eq!(
        archive
            .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
            .await
            .expect("previous complete membership remains"),
        original_members
    );
    let coverage = archive
        .family_coverage(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("read incomplete coverage");
    assert!(matches!(
        coverage.state(),
        CoverageState::Incomplete {
            sequence,
            reason: IncompleteReason::Pagination,
            received_items: 1,
            ..
        } if *sequence == partial.sequence
    ));

    let incomplete_empty = archive
        .reserve_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:04.5Z"),
            "GET /issues/1/comments",
        )
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
    assert_eq!(
        archive
            .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
            .await
            .expect("incomplete empty leaves prior membership intact"),
        original_members
    );

    let reviews = archive
        .reserve_child_family_observation(
            &thread_id,
            EvidenceFamily::Reviews,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:05Z"),
            "GET /pulls/1/reviews",
        )
        .await
        .expect("reserve reviews independently");
    assert!(reviews.reserved);
    let review_head =
        CommitSha::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect("review head SHA");
    archive
        .finish_child_family_observation(ChildFamilyObservation {
            thread: &thread_id,
            family: EvidenceFamily::Reviews,
            sequence: reviews.sequence,
            observed_at: timestamp("2026-09-20T10:00:06Z"),
            completeness: &CollectionCompleteness::Complete,
            expected_pages: Some(0),
            head_sha: Some(&review_head),
        })
        .await
        .expect("complete empty reviews");
    assert!(
        archive
            .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Reviews)
            .await
            .expect("empty reviews")
            .is_empty()
    );
    assert!(
        archive
            .pull_request_family_is_current_for_head(
                &thread_id,
                EvidenceFamily::Reviews,
                &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
                &review_head,
            )
            .await
            .expect("read review snapshot context")
    );

    let review_threads = archive
        .reserve_child_family_observation(
            &thread_id,
            EvidenceFamily::ReviewThreads,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:06.1Z"),
            "POST /graphql reviewThreads",
        )
        .await
        .expect("reserve review threads independently");
    archive
        .finish_child_family_observation(ChildFamilyObservation {
            thread: &thread_id,
            family: EvidenceFamily::ReviewThreads,
            sequence: review_threads.sequence,
            observed_at: timestamp("2026-09-20T10:00:06.2Z"),
            completeness: &CollectionCompleteness::Complete,
            expected_pages: Some(0),
            head_sha: Some(&review_head),
        })
        .await
        .expect("complete empty review-thread snapshot");
    assert!(
        archive
            .pull_request_family_is_current_for_head(
                &thread_id,
                EvidenceFamily::ReviewThreads,
                &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
                &review_head,
            )
            .await
            .expect("read review-thread snapshot context")
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
