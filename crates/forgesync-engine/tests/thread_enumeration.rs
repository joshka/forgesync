use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use forgesync_core::{CoverageState, EvidenceFamily, FailureKind, GitHubHost};
use forgesync_engine::{RepositorySelector, enumerate_repository_threads};
use forgesync_github::{GitHubClient, GitHubClientConfig};
use forgesync_store::{
    Archive, RepositoryThreadScanStatus, ThreadQuery, ThreadSort, ThreadStateFilter,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[tokio::test]
async fn page_two_failure_keeps_page_one_and_records_incomplete_coverage() {
    let server = MockServer::start().await;
    mount_repository(&server, "old", "name", "new", 1).await;
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
    assert!(
        report
            .scan
            .next_page_url
            .as_deref()
            .is_some_and(|url| url.contains("page=2"))
    );
    assert_eq!(
        report.scan.failure.as_ref().map(|failure| failure.kind),
        Some(FailureKind::ProviderResponse)
    );

    let stored_repository = archive
        .find_repository(&GitHubHost::parse("github.com").unwrap(), "new", "name")
        .await
        .expect("look up renamed repository")
        .expect("repository exists");
    let page = archive
        .query_threads(&ThreadQuery {
            repositories: vec![stored_repository.id.clone()],
            kind: None,
            state: ThreadStateFilter::All,
            match_expression: None,
            sort: ThreadSort::Updated,
            limit: NonZeroU32::new(20).unwrap(),
            offset: 0,
        })
        .await
        .expect("read stored page");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].discussion.title, "page one");
    let coverage = archive
        .family_coverage(&page.items[0].discussion.id, EvidenceFamily::Threads)
        .await
        .expect("read thread coverage");
    assert!(matches!(coverage.state(), CoverageState::Complete { .. }));

    archive.close().await;
    remove_archive(&archive_path);
}

#[tokio::test]
async fn replay_reuses_canonical_threads_without_duplicate_rows() {
    let server = MockServer::start().await;
    mount_repository(&server, "owner", "repo", "owner", 2).await;
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
    for _ in 0..2 {
        let report =
            enumerate_repository_threads(&archive, &client, &selector, &CancellationToken::new())
                .await
                .expect("complete report");
        assert_eq!(report.scan.status, RepositoryThreadScanStatus::Complete);
        assert_eq!(report.scan.pages_completed, 2);
        assert_eq!(report.scan.threads_seen, 2);
    }

    let page = archive
        .query_threads(&ThreadQuery {
            repositories: Vec::new(),
            kind: None,
            state: ThreadStateFilter::All,
            match_expression: None,
            sort: ThreadSort::Updated,
            limit: NonZeroU32::new(20).unwrap(),
            offset: 0,
        })
        .await
        .expect("read stored threads");
    assert_eq!(page.items.len(), 2);
    archive.close().await;
    remove_archive(&archive_path);
}

fn client(server: &MockServer) -> GitHubClient {
    let api_base_url = format!("{}/api/v3/", server.uri())
        .parse()
        .expect("local API URL");
    GitHubClient::new(GitHubClientConfig::new(api_base_url), None).expect("GitHub client")
}

async fn mount_repository(
    server: &MockServer,
    old_owner: &str,
    name: &str,
    new_owner: &str,
    expected_requests: u64,
) {
    let old_path = format!("/api/v3/repos/{old_owner}/{name}");
    let new_path = format!("/api/v3/repos/{new_owner}/{name}");
    if old_path != new_path {
        Mock::given(method("GET"))
            .and(path(old_path))
            .respond_with(ResponseTemplate::new(301).insert_header("Location", new_path.clone()))
            .expect(1)
            .mount(server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path(new_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 41,
            "name": name,
            "full_name": format!("{new_owner}/{name}"),
            "owner": { "login": new_owner },
            "default_branch": "main",
            "updated_at": "2026-09-20T12:00:00Z"
        })))
        .expect(expected_requests)
        .mount(server)
        .await;
}

fn issue(id: u64, number: u64, title: &str) -> serde_json::Value {
    json!({
        "id": id,
        "number": number,
        "state": "open",
        "title": title,
        "body": null,
        "created_at": "2026-09-18T08:00:00Z",
        "updated_at": "2026-09-20T09:30:00Z",
        "closed_at": null,
        "html_url": format!("https://github.com/owner/repo/issues/{number}"),
        "labels": [],
        "assignees": [],
        "user": { "login": "maintainer" }
    })
}

fn temporary_archive_path() -> PathBuf {
    let next = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-thread-enumeration-{}-{next}.sqlite",
        std::process::id()
    ))
}

fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}
