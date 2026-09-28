use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use forgesync_core::{
    CollectionCompleteness, Comment, CommentId, CoverageState, Discussion, EvidenceFamily,
    GitHubHost, Observation, ProviderData, ProviderId, Repository, RepositoryId, SourceClock,
    SourceState, ThreadId, ThreadKind, ThreadNumber, ThreadReference, UtcTimestamp,
};
use forgesync_store::{
    Archive, StagedItem, StoreError, ThreadQuery, ThreadSort, ThreadStateFilter,
    ThreadTimelineEvent,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[tokio::test]
async fn list_search_and_status_use_stable_filters_pagination_and_coverage() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let first_repository = repository("example", "first", "repo-first");
    let second_repository = repository("example", "second", "repo-second");
    archive
        .upsert_repository(&first_repository)
        .await
        .expect("store first repository");
    archive
        .upsert_repository(&second_repository)
        .await
        .expect("store second repository");

    let first_thread = thread_id(&first_repository.id, "thread-1", 1);
    let second_thread = thread_id(&first_repository.id, "thread-2", 2);
    let third_thread = thread_id(&second_repository.id, "thread-3", 3);
    apply_thread(
        &archive,
        discussion(
            &first_thread,
            ThreadKind::Issue,
            SourceState::Open,
            "Needle in title",
            Some("body text"),
            "2026-09-20T10:00:00Z",
        ),
    )
    .await;
    apply_thread(
        &archive,
        discussion(
            &second_thread,
            ThreadKind::PullRequest,
            SourceState::Closed,
            "Other title",
            Some("needle in discussion"),
            "2026-09-20T10:00:02Z",
        ),
    )
    .await;
    apply_thread(
        &archive,
        discussion(
            &third_thread,
            ThreadKind::Issue,
            SourceState::Open,
            "Needle elsewhere",
            None,
            "2026-09-20T10:00:01Z",
        ),
    )
    .await;

    let found_repository = archive
        .find_repository(
            &GitHubHost::parse("github.com").expect("host"),
            "EXAMPLE",
            "FIRST",
        )
        .await
        .expect("repository lookup")
        .expect("case-insensitive repository lookup");
    assert_eq!(found_repository.id, first_repository.id);

    let query = ThreadQuery {
        repositories: vec![first_repository.id.clone()],
        kind: None,
        state: ThreadStateFilter::All,
        match_expression: Some("\"needle\"".to_owned()),
        sort: ThreadSort::Updated,
        limit: NonZeroU32::new(1).expect("positive limit"),
        offset: 0,
    };
    let page = archive
        .query_threads(&query)
        .await
        .expect("search local archive");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].repository.full_name, "example/first");
    assert_eq!(page.items[0].discussion.id.number().get(), 2);
    assert_eq!(page.next_offset, Some(1));
    assert!(matches!(
        page.items[0]
            .coverage
            .iter()
            .find(|coverage| coverage.family() == EvidenceFamily::Threads)
            .expect("thread coverage")
            .state(),
        CoverageState::Complete { .. }
    ));

    let next_page = archive
        .query_threads(&ThreadQuery {
            offset: 1,
            ..query.clone()
        })
        .await
        .expect("read second stable page");
    assert_eq!(next_page.items.len(), 1);
    assert_eq!(next_page.items[0].discussion.id.number().get(), 1);
    assert_eq!(next_page.next_offset, None);

    let closed = archive
        .query_threads(&ThreadQuery {
            repositories: vec![first_repository.id.clone()],
            kind: Some(ThreadKind::PullRequest),
            state: ThreadStateFilter::Closed,
            match_expression: None,
            sort: ThreadSort::Created,
            limit: NonZeroU32::new(10).expect("positive limit"),
            offset: 0,
        })
        .await
        .expect("filter closed pull requests");
    assert_eq!(closed.items.len(), 1);
    assert_eq!(closed.items[0].discussion.id.number().get(), 2);

    let unmatched = archive
        .query_threads(&ThreadQuery {
            repositories: vec![first_repository.id.clone()],
            kind: None,
            state: ThreadStateFilter::All,
            match_expression: Some("\"never-matches\"".to_owned()),
            sort: ThreadSort::Relevance,
            limit: NonZeroU32::new(10).expect("positive limit"),
            offset: 0,
        })
        .await
        .expect("empty search is successful");
    assert!(unmatched.items.is_empty());
    assert_eq!(unmatched.coverage[0].applicable_threads, 2);
    assert_eq!(unmatched.coverage[0].complete, 2);
    assert_eq!(unmatched.coverage[1].missing, 2);
    assert_eq!(unmatched.coverage[2].applicable_threads, 1);
    assert_eq!(unmatched.coverage[2].missing, 1);

    let status = archive.archive_status().await.expect("read archive status");
    assert_eq!(status.repositories, 2);
    assert_eq!(status.threads, 3);
    assert_eq!(status.issues, 2);
    assert_eq!(status.pull_requests, 1);
    assert_eq!(status.coverage[2].applicable_threads, 1);

    let malformed_query = archive
        .query_threads(&ThreadQuery {
            match_expression: Some("NEAR(".to_owned()),
            ..query
        })
        .await;
    assert!(matches!(
        malformed_query,
        Err(StoreError::InvalidSearchQuery)
    ));

    archive.close().await;
    let read_only = Archive::open_read_only(&path)
        .await
        .expect("open archive read-only");
    assert!(read_only.is_read_only());
    let before = read_only
        .archive_status()
        .await
        .expect("status before query");
    read_only
        .query_threads(&ThreadQuery {
            repositories: vec![first_repository.id],
            kind: None,
            state: ThreadStateFilter::All,
            match_expression: Some("\"needle\"".to_owned()),
            sort: ThreadSort::Relevance,
            limit: NonZeroU32::new(10).expect("positive limit"),
            offset: 0,
        })
        .await
        .expect("query through a read-only archive");
    let after = read_only
        .archive_status()
        .await
        .expect("status after query");
    assert_eq!(before, after);
    read_only.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn fts_index_tracks_updates_and_removed_text_transactionally() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository("example", "search", "repo-search");
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let thread = thread_id(&repository.id, "thread-search", 1);
    apply_thread(
        &archive,
        discussion(
            &thread,
            ThreadKind::Issue,
            SourceState::Open,
            "Old title",
            Some("distinctive obsolete content"),
            "2026-09-20T10:00:00Z",
        ),
    )
    .await;

    assert_eq!(query(&archive, "\"obsolete\"").await.items.len(), 1);
    apply_thread(
        &archive,
        discussion(
            &thread,
            ThreadKind::Issue,
            SourceState::Open,
            "Replacement title",
            None,
            "2026-09-20T10:00:01Z",
        ),
    )
    .await;
    assert!(query(&archive, "\"obsolete\"").await.items.is_empty());
    assert_eq!(query(&archive, "\"replacement\"").await.items.len(), 1);

    archive.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn explicit_migration_builds_search_index_for_existing_threads() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository("example", "migration", "repo-migration");
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let thread = thread_id(&repository.id, "thread-migration", 4);
    apply_thread(
        &archive,
        discussion(
            &thread,
            ThreadKind::Issue,
            SourceState::Open,
            "Migration target",
            Some("backfill searchable body"),
            "2026-09-20T10:00:00Z",
        ),
    )
    .await;
    archive.close().await;

    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(false)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open archive to simulate schema v2");
    sqlx::query("DROP TRIGGER threads_search_insert")
        .execute(&pool)
        .await
        .expect("drop v3 insert trigger");
    sqlx::query("DROP TRIGGER threads_search_delete")
        .execute(&pool)
        .await
        .expect("drop v3 delete trigger");
    sqlx::query("DROP TRIGGER threads_search_update")
        .execute(&pool)
        .await
        .expect("drop v3 update trigger");
    sqlx::query("DROP TABLE thread_search")
        .execute(&pool)
        .await
        .expect("drop v3 search index");
    sqlx::query("DROP TABLE repository_thread_scans")
        .execute(&pool)
        .await
        .expect("drop v4 repository scans");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 3")
        .execute(&pool)
        .await
        .expect("mark archive at schema v2");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 4")
        .execute(&pool)
        .await
        .expect("mark archive at schema v2");
    pool.close().await;

    assert!(matches!(
        Archive::open_read_only(&path).await,
        Err(StoreError::MigrationRequired {
            current: 2,
            supported: 4
        })
    ));
    let migration = Archive::migrate(&path).await.expect("migrate archive");
    assert_eq!(migration.applied_migrations.len(), 2);
    let migrated = Archive::open_read_only(&path)
        .await
        .expect("open migrated archive");
    assert_eq!(query(&migrated, "\"backfill\"").await.items.len(), 1);
    migrated.close().await;
    remove_archive(&path);
}

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

async fn query(archive: &Archive, expression: &str) -> forgesync_store::ThreadPage {
    archive
        .query_threads(&ThreadQuery {
            repositories: Vec::new(),
            kind: None,
            state: ThreadStateFilter::All,
            match_expression: Some(expression.to_owned()),
            sort: ThreadSort::Relevance,
            limit: NonZeroU32::new(10).expect("positive limit"),
            offset: 0,
        })
        .await
        .expect("query thread page")
}

async fn apply_thread(archive: &Archive, discussion: Discussion) {
    let updated_at = discussion.updated_at;
    let sequence = archive
        .reserve_observation_sequence(updated_at)
        .await
        .expect("reserve sequence");
    let raw_source_clock = updated_at.format_rfc3339().expect("format timestamp");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        discussion,
        SourceClock::from_raw(Some(&raw_source_clock)),
        updated_at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply thread observation");
}

fn discussion(
    thread: &ThreadId,
    kind: ThreadKind,
    state: SourceState,
    title: &str,
    body: Option<&str>,
    updated_at: &str,
) -> Discussion {
    Discussion {
        id: thread.clone(),
        kind,
        state,
        title: title.to_owned(),
        body: body.map(str::to_owned),
        html_url: None,
        created_at: timestamp("2026-09-19T10:00:00Z"),
        updated_at: timestamp(updated_at),
        closed_at: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        provider_data: ProviderData::new(),
    }
}

fn repository(owner: &str, name: &str, provider_id: &str) -> Repository {
    Repository {
        id: RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new(provider_id).expect("repository provider ID"),
        ),
        owner: owner.to_owned(),
        name: name.to_owned(),
        full_name: format!("{owner}/{name}"),
        default_branch: Some("main".to_owned()),
        updated_at: Some(timestamp("2026-09-20T10:00:00Z")),
        provider_data: ProviderData::new(),
    }
}

fn thread_id(repository: &RepositoryId, provider_id: &str, number: u64) -> ThreadId {
    ThreadId::new(
        repository.clone(),
        ProviderId::new(provider_id).expect("thread provider ID"),
        ThreadNumber::new(number).expect("thread number"),
    )
}

fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
}

fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-store-{}-{sequence}.sqlite",
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
