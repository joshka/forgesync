//! # Complete empty pull-request snapshots
//!
//! Named review and review-thread cases independently publish empty evidence for one explicit head.
//! Complete emptiness must remain eligible for head-aware reuse rather than look like missing work.
//! The parent and each acquisition are created directly; fixtures only construct values.
//! Both families use identical completeness and head contracts without runtime scenario branching.
//!
//! Membership and current-head eligibility are read through the public archive API.
//! This checks snapshot publication, not provider GraphQL shape or engine pagination.
//! Fixed times expose source/acquisition coordinates without requiring a process clock.
//! The on-disk archive is closed before resource cleanup.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::CommitSha;
use forgesync_core::observation::{CollectionCompleteness, SourceClock};
use forgesync_store::archive::Archive;
use forgesync_store::families::ChildFamilyObservation;

use crate::fixture::{
    discussion, remove_archive, repository, temporary_archive_path, thread_id, thread_observation,
    timestamp,
};

#[rstest::rstest]
#[case::reviews(EvidenceFamily::Reviews)]
#[case::review_threads(EvidenceFamily::ReviewThreads)]
#[tokio::test]
async fn complete_empty_snapshot_is_current_for_its_head(#[case] family: EvidenceFamily) {
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

    let snapshot = archive
        .reserve_child_family_observation(
            &thread_id,
            family,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:05Z"),
            "fixture empty snapshot",
        )
        .await
        .expect("reserve reviews independently");
    assert!(snapshot.reserved);
    let review_head =
        CommitSha::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect("review head SHA");
    archive
        .finish_child_family_observation(ChildFamilyObservation {
            thread: &thread_id,
            family,
            sequence: snapshot.sequence,
            observed_at: timestamp("2026-09-20T10:00:06Z"),
            completeness: &CollectionCompleteness::Complete,
            expected_pages: Some(0),
            head_sha: Some(&review_head),
        })
        .await
        .expect("complete empty reviews");
    assert!(
        archive
            .child_family_members::<serde_json::Value>(&thread_id, family)
            .await
            .expect("empty reviews")
            .is_empty()
    );
    assert!(
        archive
            .pull_request_family_is_current_for_head(
                &thread_id,
                family,
                &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
                &review_head,
            )
            .await
            .expect("read review snapshot context")
    );

    archive.close().await;
    remove_archive(&path);
}
