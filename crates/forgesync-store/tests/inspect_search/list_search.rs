//! # List and search query cases
//!
//! These cases cover repository scope, filters, ordering, and pagination in local reads. They keep
//! SQL behavior aligned with the typed query model. A caller should receive a stable page rather
//! than reconstructing filter semantics from raw rows.

use std::num::NonZeroU32;

use forgesync_core::content::{SourceState, ThreadKind};
use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::GitHubHost;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::reads::{ThreadQuery, ThreadSort, ThreadStateFilter};

use super::{
    apply_thread, discussion, remove_archive, repository, temporary_archive_path, thread_id,
};

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
        updated_since: None,
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
            updated_since: None,
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
            updated_since: None,
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
            updated_since: None,
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
