//! # Replayed scans preserve canonical discussion identity and content
//!
//! Both scans traverse the same two issue pages. Separate mock expectations require each page to be
//! fetched twice, and scan sequences must increase. The first local read identifies both
//! discussions by number and title in deterministic updated-time order.
//!
//! After replay the same two complete discussion payloads remain; matching only a total count would
//! miss replacement or duplication of one identity. The terminal scan has no continuation or
//! failure and is compared directly with persisted state.
//!
//! The dependent initial/replay steps stay together because the preservation assertion needs its
//! baseline. Fixtures do not execute enumeration or compute expected scan results.

use std::num::NonZeroU32;

use forgesync_engine::enumeration::enumerate_repository_threads;
use forgesync_engine::reference::RepositorySelector;
use forgesync_store::archive::Archive;
use forgesync_store::enumeration::RepositoryThreadScanStatus;
use forgesync_store::reads::{ThreadQuery, ThreadSort, ThreadStateFilter};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::fixture::{client, issue, mount_repository, remove_archive, temporary_archive_path};

#[tokio::test]
async fn replay_reuses_canonical_threads_without_duplicate_rows() {
    let server = MockServer::start().await;
    mount_repository(&server, "owner", "repo", 2).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "all"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", "<?page=2>; rel=\"next\"")
                .set_body_json(json!([issue(92, 12, "first")])),
        )
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([issue(93, 13, "second")])))
        .expect(2)
        .mount(&server)
        .await;

    let archive_path = temporary_archive_path();
    let archive = Archive::create(&archive_path)
        .await
        .expect("create archive");
    let client = client(&server);
    let selector = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("selector");
    let initial =
        enumerate_repository_threads(&archive, &client, &selector, &CancellationToken::new())
            .await
            .expect("initial complete scan");
    assert_eq!(initial.scan.status, RepositoryThreadScanStatus::Complete);
    assert_eq!(initial.scan.pages_completed, 2);
    assert_eq!(initial.scan.threads_seen, 2);

    let query = ThreadQuery {
        repositories: Vec::new(),
        kind: None,
        state: ThreadStateFilter::All,
        match_expression: None,
        updated_since: None,
        sort: ThreadSort::Updated,
        limit: NonZeroU32::new(20).expect("positive page limit"),
        offset: 0,
    };
    let before = archive
        .query_threads(&query)
        .await
        .expect("read initial threads");
    assert_eq!(before.items.len(), 2);
    assert_eq!(before.items[0].discussion.id.number().get(), 13);
    assert_eq!(before.items[0].discussion.title, "second");
    assert_eq!(before.items[1].discussion.id.number().get(), 12);
    assert_eq!(before.items[1].discussion.title, "first");

    let replay =
        enumerate_repository_threads(&archive, &client, &selector, &CancellationToken::new())
            .await
            .expect("replayed complete scan");
    assert_eq!(replay.scan.status, RepositoryThreadScanStatus::Complete);
    assert_eq!(replay.scan.pages_completed, 2);
    assert_eq!(replay.scan.threads_seen, 2);
    assert!(replay.scan.sequence > initial.scan.sequence);

    let page = archive
        .query_threads(&query)
        .await
        .expect("read replayed threads");
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].discussion, before.items[0].discussion);
    assert_eq!(page.items[1].discussion, before.items[1].discussion);
    assert_eq!(replay.scan.next_page_url, None);
    assert_eq!(replay.scan.failure, None);
    let persisted_scan = archive
        .repository_thread_scan(&replay.repository.id)
        .await
        .expect("read persisted replay scan");
    assert_eq!(persisted_scan, Some(replay.scan));
    archive.close().await;
    remove_archive(&archive_path);
}
