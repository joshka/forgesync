//! # A later page failure preserves committed parent evidence
//!
//! A same-provider repository redirect resolves the requested old name to the canonical identity.
//! Page one commits one issue; page two returns a provider error. The report must retain the exact
//! continuation URL, first-page counts, incomplete scan status, and classified failure.
//!
//! The archive projection verifies the canonical repository and persisted scan against the report.
//! It also checks the retained issue number/title and complete parent coverage with one item.
//! Comment coverage stays missing because enumeration does not acquire child families.
//!
//! HTTP requests and the real enumeration call remain explicit. Fixture setup supplies clients,
//! payloads, and a stable repository response; cleanup follows explicit archive closure.

use std::num::NonZeroU32;

use forgesync_core::coverage::{CoverageState, EvidenceFamily, FailureKind};
use forgesync_core::identity::GitHubHost;
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
async fn page_two_failure_keeps_page_one_and_records_incomplete_coverage() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/old/name"))
        .respond_with(
            ResponseTemplate::new(301).insert_header("Location", "/api/v3/repos/new/name"),
        )
        .expect(1)
        .mount(&server)
        .await;
    mount_repository(&server, "new", "name", 1).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/new/name/issues"))
        .and(query_param("state", "all"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", "<?page=2>; rel=\"next\"")
                .set_body_json(json!([issue(91, 11, "page one")])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/new/name/issues"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    let archive_path = temporary_archive_path();
    let archive = Archive::create(&archive_path)
        .await
        .expect("create archive");
    let client = client(&server);
    let selector = "old/name".parse::<RepositorySelector>().expect("selector");
    let report =
        enumerate_repository_threads(&archive, &client, &selector, &CancellationToken::new())
            .await
            .expect("partial report");

    assert_eq!(report.repository.full_name, "new/name");
    assert_eq!(report.scan.status, RepositoryThreadScanStatus::Incomplete);
    assert_eq!(report.scan.pages_completed, 1);
    assert_eq!(report.scan.threads_seen, 1);
    let expected_next_page = format!("{}/api/v3/repos/new/name/issues?page=2", server.uri());
    assert_eq!(
        report.scan.next_page_url.as_deref(),
        Some(expected_next_page.as_str())
    );
    assert_eq!(
        report.scan.failure.as_ref().map(|failure| failure.kind),
        Some(FailureKind::ProviderResponse)
    );

    let host = GitHubHost::parse("github.com").expect("fixture host");
    let lookup = archive
        .find_repository(&host, "new", "name")
        .await
        .expect("look up renamed repository");
    let stored_repository = lookup.expect("repository exists");
    assert_eq!(stored_repository.id, report.repository.id);
    let persisted_scan = archive
        .repository_thread_scan(&stored_repository.id)
        .await
        .expect("read persisted incomplete scan");
    assert_eq!(persisted_scan, Some(report.scan.clone()));
    let page = archive
        .query_threads(&ThreadQuery {
            repositories: vec![stored_repository.id.clone()],
            kind: None,
            state: ThreadStateFilter::All,
            match_expression: None,
            updated_since: None,
            sort: ThreadSort::Updated,
            limit: NonZeroU32::new(20).expect("positive page limit"),
            offset: 0,
        })
        .await
        .expect("read stored page");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].discussion.id.number().get(), 11);
    assert_eq!(page.items[0].discussion.title, "page one");
    let coverage = archive
        .family_coverage(&page.items[0].discussion.id, EvidenceFamily::Threads)
        .await
        .expect("read thread coverage");
    assert!(matches!(
        coverage.state(),
        CoverageState::Complete { item_count: 1, .. }
    ));

    let comments = archive
        .family_coverage(&page.items[0].discussion.id, EvidenceFamily::Comments)
        .await
        .expect("read unselected comment coverage");
    assert_eq!(comments.state(), &CoverageState::Missing);

    archive.close().await;
    remove_archive(&archive_path);
}
