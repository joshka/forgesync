use std::cmp::Ordering;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::{
    CommitSha, GitHubHost, ObservationSequence, ProviderId, RepositoryId, ThreadId, ThreadNumber,
};
use forgesync_core::observation::{
    CollectionCompleteness, IncompleteReason, Observation, SourceClock,
};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::{
    Archive, ChildFamilyObservation, ObservationDisposition, StagedItem, StoreError,
    compare_observation_order, compare_revision_observation_order,
    observation_sequence_order_value,
};
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[tokio::test]
async fn parent_observations_keep_separate_source_and_evidence_high_waters() {
    let path = temporary_archive_path();
    let (archive, repository_id, thread_id) = create_archive_with_repository(&path).await;
    let first_sequence = reserve(&archive, "2026-09-20T10:00:00Z").await;
    let second_sequence = reserve(&archive, "2026-09-20T10:00:01Z").await;
    let third_sequence = reserve(&archive, "2026-09-20T10:00:02Z").await;
    let fourth_sequence = reserve(&archive, "2026-09-20T10:00:03Z").await;

    let first = discussion(&repository_id, &thread_id, "2026-09-20T10:00:00Z", "first");
    let applied = archive
        .apply_thread_observation(&thread_observation(
            first.clone(),
            "2026-09-20T10:00:00Z",
            "2026-09-20T10:00:00Z",
            first_sequence,
            CollectionCompleteness::Complete,
        ))
        .await
        .expect("apply initial complete observation");
    assert_eq!(applied.disposition, ObservationDisposition::Applied);
    assert_eq!(applied.high_water_sequence, first_sequence);
    assert_eq!(applied.evidence_sequence, Some(first_sequence));

    let replayed_incomplete = archive
        .apply_thread_observation(&thread_observation(
            first.clone(),
            "2026-09-20T10:00:00Z",
            "2026-09-20T10:00:01Z",
            third_sequence,
            incomplete(1),
        ))
        .await
        .expect("apply same-payload incomplete generation");
    assert_eq!(
        replayed_incomplete.disposition,
        ObservationDisposition::Applied
    );
    assert_eq!(replayed_incomplete.high_water_sequence, third_sequence);
    assert_eq!(replayed_incomplete.evidence_sequence, Some(first_sequence));

    let newer_parent = discussion(&repository_id, &thread_id, "2026-09-20T10:00:01Z", "newer");
    let incomplete_newer = archive
        .apply_thread_observation(&thread_observation(
            newer_parent.clone(),
            "2026-09-20T10:00:01Z",
            "2026-09-20T10:00:02Z",
            fourth_sequence,
            incomplete(1),
        ))
        .await
        .expect("apply newer incomplete parent");
    assert_eq!(incomplete_newer.high_water_sequence, fourth_sequence);
    assert_eq!(incomplete_newer.evidence_sequence, Some(first_sequence));

    let hydrated = archive
        .apply_thread_observation(&thread_observation(
            newer_parent.clone(),
            "2026-09-20T10:00:01Z",
            "2026-09-20T10:00:03Z",
            second_sequence,
            CollectionCompleteness::Complete,
        ))
        .await
        .expect("hydrate same source below the parent high-water mark");
    assert_eq!(hydrated.disposition, ObservationDisposition::Applied);
    assert_eq!(hydrated.high_water_sequence, fourth_sequence);
    assert_eq!(hydrated.evidence_sequence, Some(second_sequence));

    let delayed = discussion(
        &repository_id,
        &thread_id,
        "2026-09-20T10:00:01Z",
        "delayed conflicting payload",
    );
    let skipped = archive
        .apply_thread_observation(&thread_observation(
            delayed,
            "2026-09-20T10:00:01Z",
            "2026-09-20T10:00:04Z",
            third_sequence,
            CollectionCompleteness::Complete,
        ))
        .await
        .expect("skip delayed conflicting hydration");
    assert_eq!(skipped.disposition, ObservationDisposition::Skipped);
    assert_eq!(skipped.high_water_sequence, fourth_sequence);
    assert_eq!(skipped.evidence_sequence, Some(second_sequence));

    let coverage = archive
        .family_coverage(&thread_id, EvidenceFamily::Threads)
        .await
        .expect("read thread coverage");
    assert!(matches!(
        coverage.state(),
        CoverageState::Complete { sequence, .. } if *sequence == second_sequence
    ));
    archive.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn tied_conflicts_are_rejected_and_identical_observations_are_idempotent() {
    let path = temporary_archive_path();
    let (archive, repository_id, thread_id) = create_archive_with_repository(&path).await;
    let sequence = reserve(&archive, "2026-09-20T10:00:00Z").await;
    let observation = thread_observation(
        discussion(&repository_id, &thread_id, "2026-09-20T10:00:00Z", "same"),
        "2026-09-20T10:00:00Z",
        "2026-09-20T10:00:00Z",
        sequence,
        CollectionCompleteness::Complete,
    );

    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply first observation");
    let replay = archive
        .apply_thread_observation(&observation)
        .await
        .expect("replay same observation");
    assert_eq!(replay.disposition, ObservationDisposition::Replayed);

    let conflicting = thread_observation(
        discussion(
            &repository_id,
            &thread_id,
            "2026-09-20T10:00:00Z",
            "conflict",
        ),
        "2026-09-20T10:00:00Z",
        "2026-09-20T10:00:00Z",
        sequence,
        CollectionCompleteness::Complete,
    );
    assert!(matches!(
        archive.apply_thread_observation(&conflicting).await,
        Err(StoreError::ConflictingObservation)
    ));

    let current = read_current_thread_title(&path).await;
    assert_eq!(current, "same");
    archive.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn malformed_source_clocks_are_ambiguous_but_revision_sequences_remain_distinct() {
    let path = temporary_archive_path();
    let (archive, repository_id, thread_id) = create_archive_with_repository(&path).await;
    let first_sequence = reserve(&archive, "2026-09-20T10:00:00Z").await;
    let second_sequence = reserve(&archive, "2026-09-20T10:00:01Z").await;
    let first = thread_observation(
        discussion(&repository_id, &thread_id, "2026-09-20T10:00:00Z", "first"),
        "not-a-time-a",
        "2026-09-20T10:00:00Z",
        first_sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&first)
        .await
        .expect("apply malformed source clock");
    let second = thread_observation(
        discussion(&repository_id, &thread_id, "2026-09-20T10:00:00Z", "second"),
        "not-a-time-b",
        "2026-09-20T10:00:01Z",
        second_sequence,
        CollectionCompleteness::Complete,
    );
    assert!(matches!(
        archive.apply_thread_observation(&second).await,
        Err(StoreError::AmbiguousObservationClocks { .. })
    ));

    let revision_order = compare_revision_observation_order(
        &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
        Some(second_sequence),
        &SourceClock::from_raw(Some("2026-09-20T10:00:01Z")),
        Some(first_sequence),
    )
    .expect("revision evidence compares acquisition sequences first");
    assert_eq!(revision_order, Ordering::Greater);
    archive.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn child_families_stage_pages_and_only_complete_results_replace_membership() {
    let path = temporary_archive_path();
    let (archive, repository_id, thread_id) = create_archive_with_repository(&path).await;
    let thread_sequence = reserve(&archive, "2026-09-20T10:00:00Z").await;
    archive
        .apply_thread_observation(&thread_observation(
            discussion(&repository_id, &thread_id, "2026-09-20T10:00:00Z", "thread"),
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
        .finish_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            comments.sequence,
            timestamp("2026-09-20T10:00:02Z"),
            &CollectionCompleteness::Complete,
            Some(1),
        )
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
        .finish_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            partial.sequence,
            timestamp("2026-09-20T10:00:04Z"),
            &incomplete(1),
            None,
        )
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
        .finish_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            incomplete_empty.sequence,
            timestamp("2026-09-20T10:00:04.6Z"),
            &incomplete(0),
            None,
        )
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
        .finish_child_family_observation_with_context(ChildFamilyObservation {
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
        .finish_child_family_observation_with_context(ChildFamilyObservation {
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
        .finish_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            stale_comments.sequence,
            timestamp("2026-09-20T10:00:07.2Z"),
            &CollectionCompleteness::Complete,
            Some(0),
        )
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
        .finish_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            empty_comments.sequence,
            timestamp("2026-09-20T10:00:08Z"),
            &CollectionCompleteness::Complete,
            Some(0),
        )
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

#[tokio::test]
async fn failed_membership_and_coverage_transaction_keeps_both_old_values() {
    let path = temporary_archive_path();
    let (archive, repository_id, thread_id) = create_archive_with_repository(&path).await;
    let thread_sequence = reserve(&archive, "2026-09-20T10:00:00Z").await;
    archive
        .apply_thread_observation(&thread_observation(
            discussion(&repository_id, &thread_id, "2026-09-20T10:00:00Z", "thread"),
            "2026-09-20T10:00:00Z",
            "2026-09-20T10:00:00Z",
            thread_sequence,
            CollectionCompleteness::Complete,
        ))
        .await
        .expect("apply parent");
    let first = archive
        .reserve_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:01Z"),
            "comments",
        )
        .await
        .expect("reserve first collection");
    archive
        .stage_child_family_page(
            &thread_id,
            EvidenceFamily::Comments,
            first.sequence,
            0,
            &[item("old", json!({"body":"old"}))],
        )
        .await
        .expect("stage first collection");
    archive
        .finish_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            first.sequence,
            timestamp("2026-09-20T10:00:02Z"),
            &CollectionCompleteness::Complete,
            Some(1),
        )
        .await
        .expect("commit first collection");

    let next = archive
        .reserve_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            &SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            timestamp("2026-09-20T10:00:03Z"),
            "comments",
        )
        .await
        .expect("reserve replacement collection");
    archive
        .stage_child_family_page(
            &thread_id,
            EvidenceFamily::Comments,
            next.sequence,
            0,
            &[item("new", json!({"body":"new"}))],
        )
        .await
        .expect("stage replacement");

    let trigger_pool = writable_pool(&path).await;
    sqlx::query(
        "CREATE TRIGGER reject_comment_coverage BEFORE UPDATE ON family_coverage WHEN NEW.family = 'comments' BEGIN SELECT RAISE(ABORT, 'forced coverage failure'); END",
    )
    .execute(&trigger_pool)
    .await
    .expect("install test trigger");
    assert!(
        archive
            .finish_child_family_observation(
                &thread_id,
                EvidenceFamily::Comments,
                next.sequence,
                timestamp("2026-09-20T10:00:04Z"),
                &CollectionCompleteness::Complete,
                Some(1),
            )
            .await
            .is_err()
    );

    let members = archive
        .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("old membership survives rollback");
    assert_eq!(members, vec![item("old", json!({"body":"old"}))]);
    let coverage = archive
        .family_coverage(&thread_id, EvidenceFamily::Comments)
        .await
        .expect("old coverage survives rollback");
    assert!(matches!(
        coverage.state(),
        CoverageState::Complete { sequence, item_count: 1, .. } if *sequence == first.sequence
    ));

    sqlx::query("DROP TRIGGER reject_comment_coverage")
        .execute(&trigger_pool)
        .await
        .expect("remove test trigger");
    trigger_pool.close().await;
    archive
        .finish_child_family_observation(
            &thread_id,
            EvidenceFamily::Comments,
            next.sequence,
            timestamp("2026-09-20T10:00:04Z"),
            &CollectionCompleteness::Complete,
            Some(1),
        )
        .await
        .expect("retry the still-staged generation");
    assert_eq!(
        archive
            .child_family_members::<serde_json::Value>(&thread_id, EvidenceFamily::Comments)
            .await
            .expect("read replacement membership"),
        vec![item("new", json!({"body":"new"}))]
    );

    archive.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn failed_review_thread_snapshot_rolls_back_membership_coverage_and_head_context() {
    let path = temporary_archive_path();
    let (archive, repository_id, thread_id) = create_archive_with_repository(&path).await;
    let thread_sequence = reserve(&archive, "2026-09-20T10:00:00Z").await;
    archive
        .apply_thread_observation(&thread_observation(
            discussion(&repository_id, &thread_id, "2026-09-20T10:00:00Z", "thread"),
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
        .finish_child_family_observation_with_context(ChildFamilyObservation {
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
    assert!(
        archive
            .finish_child_family_observation_with_context(ChildFamilyObservation {
                thread: &thread_id,
                family: EvidenceFamily::ReviewThreads,
                sequence: next.sequence,
                observed_at: timestamp("2026-09-20T10:00:04Z"),
                completeness: &CollectionCompleteness::Complete,
                expected_pages: Some(1),
                head_sha: Some(&head),
            })
            .await
            .is_err()
    );

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

#[test]
fn observation_ordering_covers_source_precedence_legacy_fallback_and_min_sequence() {
    let incoming = SourceClock::from_raw(Some("2026-09-20T10:00:01Z"));
    let current = SourceClock::from_raw(Some("2026-09-20T10:00:00Z"));
    let sequence_one = ObservationSequence::new(1).expect("sequence");
    let sequence_two = ObservationSequence::new(2).expect("sequence");
    assert_eq!(
        compare_observation_order(&incoming, sequence_one, &current, sequence_two)
            .expect("source order"),
        Ordering::Greater
    );
    assert_eq!(observation_sequence_order_value(i64::MIN), i64::MAX);
    assert_eq!(
        compare_revision_observation_order(&incoming, None, &current, None)
            .expect("legacy source order"),
        Ordering::Greater
    );
}

async fn create_archive_with_repository(path: &PathBuf) -> (Archive, RepositoryId, ThreadId) {
    let archive = Archive::create(path).await.expect("create archive");
    let repository_id = RepositoryId::new(
        GitHubHost::parse("github.com").expect("host"),
        ProviderId::new("repository-42").expect("repository provider ID"),
    );
    let repository = Repository {
        id: repository_id.clone(),
        owner: "example".to_owned(),
        name: "project".to_owned(),
        full_name: "example/project".to_owned(),
        default_branch: Some("main".to_owned()),
        updated_at: Some(timestamp("2026-09-20T10:00:00Z")),
        provider_data: ProviderData::new(),
    };
    archive
        .upsert_repository(&repository)
        .await
        .expect("insert repository");
    let thread_id = thread_id(&repository_id);
    (archive, repository_id, thread_id)
}

fn thread_id(repository_id: &RepositoryId) -> ThreadId {
    ThreadId::new(
        repository_id.clone(),
        ProviderId::new("thread-101").expect("thread provider ID"),
        ThreadNumber::new(101).expect("thread number"),
    )
}

fn discussion(
    repository_id: &RepositoryId,
    thread_id: &ThreadId,
    updated_at: &str,
    title: &str,
) -> Discussion {
    assert_eq!(thread_id.repository(), repository_id);
    Discussion {
        id: thread_id.clone(),
        kind: ThreadKind::Issue,
        state: SourceState::Open,
        title: title.to_owned(),
        body: Some("body".to_owned()),
        html_url: Some("https://github.com/example/project/issues/101".to_owned()),
        created_at: timestamp("2026-09-19T09:00:00Z"),
        updated_at: timestamp(updated_at),
        closed_at: None,
        labels: vec!["triage".to_owned()],
        assignees: Vec::new(),
        provider_data: ProviderData::new(),
    }
}

fn thread_observation(
    discussion: Discussion,
    source_clock: &str,
    observed_at: &str,
    sequence: ObservationSequence,
    completeness: CollectionCompleteness,
) -> Observation<Discussion> {
    Observation::new(
        EvidenceFamily::Threads,
        discussion,
        SourceClock::from_raw(Some(source_clock)),
        timestamp(observed_at),
        sequence,
        completeness,
    )
}

fn incomplete(received_items: u64) -> CollectionCompleteness {
    CollectionCompleteness::Incomplete {
        reason: IncompleteReason::Pagination,
        received_items,
    }
}

fn item(id: &str, payload: serde_json::Value) -> StagedItem<serde_json::Value> {
    StagedItem {
        id: ProviderId::new(id).expect("provider item ID"),
        payload,
    }
}

async fn reserve(archive: &Archive, started_at: &str) -> ObservationSequence {
    archive
        .reserve_observation_sequence(timestamp(started_at))
        .await
        .expect("reserve observation sequence")
}

fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
}

async fn read_current_thread_title(path: &PathBuf) -> String {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false)
                .read_only(true)
                .foreign_keys(true),
        )
        .await
        .expect("open inspection pool");
    let title = sqlx::query_scalar("SELECT title FROM threads")
        .fetch_one(&pool)
        .await
        .expect("read canonical title");
    pool.close().await;
    title
}

async fn writable_pool(path: &PathBuf) -> sqlx::SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false)
                .foreign_keys(true),
        )
        .await
        .expect("open trigger pool")
}

fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, AtomicOrdering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-observations-{}-{sequence}.sqlite",
        std::process::id()
    ))
}

fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}
