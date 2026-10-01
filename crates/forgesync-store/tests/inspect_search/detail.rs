//! # Thread detail projection cases
//!
//! The scenario publishes one issue and a complete one-comment collection through public archive
//! operations, then reads them by repository and thread number. Its assertions connect normalized
//! membership to the current chronological timeline and issue-applicable family coverage.
//!
//! Comment identity, payload, event time, and event order are compared explicitly.
//! Pull-request-only collections must remain absent for this issue; the coverage catalog includes
//! only the parent and comments. Complete membership is supplied by finalization rather than
//! inferred from row count.
//!
//! Construction fixtures supply source values without performing acquisition. Reservation, staging,
//! publication, detail retrieval, closure, and cleanup remain visible in this linear scenario.
//! Partial membership and rollback belong to observation transaction suites. This case establishes
//! current detail projection, not full source revision history or provider normalization.

use forgesync_core::content::{Comment, SourceState, ThreadKind};
use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::{CommentId, ProviderId, ThreadNumber, ThreadReference};
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_core::provider_data::ProviderData;
use forgesync_store::archive::Archive;
use forgesync_store::families::{ChildFamilyObservation, ChildFamilyPage, ChildFamilyRequest};
use forgesync_store::observations::StagedItem;
use forgesync_store::reads::{ThreadTimelineEntry, ThreadTimelineEvent};

use crate::fixture::{
    discussion, remove_archive, repository, temporary_archive_path, thread_id, timestamp,
};

#[tokio::test]
async fn thread_detail_returns_typed_current_evidence_and_coverage() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let lease = crate::common::lease(&archive).await;
    let repository = repository("example", "detail", "repo-detail");
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let thread = thread_id(&repository.id, "thread-detail", 9);
    let content = discussion(
        &thread,
        ThreadKind::Issue,
        SourceState::Open,
        "Detailed issue",
        Some("body"),
        "2026-09-20T10:00:00Z",
    );
    let observed_at = content.updated_at;
    let sequence = archive
        .reserve_observation_sequence(observed_at)
        .await
        .expect("reserve sequence");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        content,
        SourceClock::Valid(observed_at),
        observed_at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation, None)
        .await
        .expect("apply thread observation");
    let reservation = archive
        .reserve_child_family_observation_fenced(
            ChildFamilyRequest {
                thread: &thread,
                family: EvidenceFamily::Comments,
                source_clock: &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
                started_at: timestamp("2026-09-20T10:00:01Z"),
                request_scope: "GET /issues/9/comments",
            },
            &lease,
        )
        .await
        .expect("reserve comments");
    let comment_id = ProviderId::new("comment-1").expect("comment provider ID");
    let comment = Comment {
        id: CommentId::new(thread.clone(), comment_id.clone()),
        review_id: None,
        author: Some("maintainer".to_owned()),
        body: "current comment".to_owned(),
        created_at: timestamp("2026-09-20T10:00:01Z"),
        updated_at: None,
        provider_data: ProviderData::new(),
    };
    archive
        .stage_child_family_page_fenced(
            ChildFamilyPage {
                thread: &thread,
                family: EvidenceFamily::Comments,
                sequence: reservation.sequence,
                page_index: 0,
                items: &[StagedItem {
                    id: comment_id.clone(),
                    payload: comment.clone(),
                }],
            },
            &lease,
        )
        .await
        .expect("stage current comment");
    archive
        .finish_child_family_observation_fenced(
            ChildFamilyObservation {
                thread: &thread,
                family: EvidenceFamily::Comments,
                sequence: reservation.sequence,
                observed_at: timestamp("2026-09-20T10:00:02Z"),
                completeness: &CollectionCompleteness::Complete,
                expected_pages: Some(1),
                head_sha: None,
            },
            &lease,
        )
        .await
        .expect("complete comments");

    let detail = archive
        .thread_detail(&ThreadReference::new(
            repository.id.clone(),
            ThreadNumber::new(9).expect("thread number"),
        ))
        .await
        .expect("read thread detail");
    assert_eq!(detail.summary.discussion.title, "Detailed issue");
    assert_eq!(
        detail.comments,
        vec![StagedItem {
            id: comment_id,
            payload: comment.clone(),
        }]
    );
    assert!(detail.pull_request_metadata.is_empty());
    assert!(detail.reviews.is_empty());
    assert!(detail.review_threads.is_empty());
    assert_eq!(detail.summary.coverage.len(), 2);
    let parent_coverage = &detail.summary.coverage[0];
    let comment_coverage = &detail.summary.coverage[1];
    assert_eq!(parent_coverage.family(), EvidenceFamily::Threads);
    assert_eq!(comment_coverage.family(), EvidenceFamily::Comments);
    assert_eq!(
        detail.timeline,
        vec![
            ThreadTimelineEntry {
                occurred_at: Some(detail.summary.discussion.created_at),
                event: ThreadTimelineEvent::ThreadCreated {
                    thread: thread.clone(),
                    title: "Detailed issue".to_owned(),
                },
            },
            ThreadTimelineEntry {
                occurred_at: Some(comment.created_at),
                event: ThreadTimelineEvent::Comment { comment },
            },
        ]
    );
    assert!(matches!(
        comment_coverage.state(),
        CoverageState::Complete { item_count: 1, .. }
    ));

    archive.close().await;
    remove_archive(&path);
}
