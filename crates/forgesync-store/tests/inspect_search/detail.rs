use super::{
    Archive, CollectionCompleteness, Comment, CommentId, CoverageState, EvidenceFamily,
    ProviderData, ProviderId, SourceClock, SourceState, StagedItem, ThreadKind, ThreadNumber,
    ThreadReference, ThreadTimelineEvent, apply_thread, discussion, remove_archive, repository,
    temporary_archive_path, thread_id, timestamp,
};

#[tokio::test]
async fn thread_detail_returns_typed_current_evidence_and_coverage() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository("example", "detail", "repo-detail");
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let thread = thread_id(&repository.id, "thread-detail", 9);
    apply_thread(
        &archive,
        discussion(
            &thread,
            ThreadKind::Issue,
            SourceState::Open,
            "Detailed issue",
            Some("body"),
            "2026-09-20T10:00:00Z",
        ),
    )
    .await;
    let reservation = archive
        .reserve_child_family_observation(
            &thread,
            EvidenceFamily::Comments,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:01Z"),
            "GET /issues/9/comments",
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
        .stage_child_family_page(
            &thread,
            EvidenceFamily::Comments,
            reservation.sequence,
            0,
            &[StagedItem {
                id: comment_id,
                payload: comment.clone(),
            }],
        )
        .await
        .expect("stage current comment");
    archive
        .finish_child_family_observation(
            &thread,
            EvidenceFamily::Comments,
            reservation.sequence,
            timestamp("2026-09-20T10:00:02Z"),
            &CollectionCompleteness::Complete,
            Some(1),
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
    assert_eq!(detail.comments.len(), 1);
    assert_eq!(detail.comments[0].payload, comment);
    assert_eq!(detail.summary.coverage.len(), 2);
    assert_eq!(detail.timeline.len(), 2);
    assert!(matches!(
        detail.timeline[0].event,
        ThreadTimelineEvent::ThreadCreated { .. }
    ));
    assert!(matches!(
        detail.timeline[1].event,
        ThreadTimelineEvent::Comment { .. }
    ));
    assert!(matches!(
        detail.summary.coverage[1].state(),
        CoverageState::Complete { item_count: 1, .. }
    ));

    archive.close().await;
    remove_archive(&path);
}
