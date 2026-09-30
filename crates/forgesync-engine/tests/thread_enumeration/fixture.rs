//! # Enumeration payload construction and archive lifetime
//!
//! These helpers construct a credential-free client pointed at the local HTTP server, static issue
//! JSON, and the repository response shared by both scenarios. Page links, page failures, and the
//! engine enumeration operation stay in the scenario files.
//!
//! Repository mounting records an explicit request-count expectation; it performs no provider
//! acquisition or archive writes. Owner/name determine the fixture route and canonical response,
//! while expected requests express each scenario's initial-versus-replay setup.
//!
//! Filename allocation provides process-local uniqueness. Cleanup removes a closed database and its
//! WAL sidecars without changing scenario assertions or masking a failed workflow.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use forgesync_github::transport::{GitHubClient, GitHubClientConfig};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Distinguishes temporary archive paths within this test process.
static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

/// Creates the provider client for the test server without credentials.
pub fn client(server: &MockServer) -> GitHubClient {
    let api_base_url = format!("{}/api/v3/", server.uri())
        .parse()
        .expect("local API URL");
    GitHubClient::new(GitHubClientConfig::new(api_base_url), None).expect("GitHub client")
}

/// Installs only a stable repository response; redirect scenarios set up their own transport.
pub async fn mount_repository(
    server: &MockServer,
    owner: &str,
    name: &str,
    expected_requests: u64,
) {
    let repository_path = format!("/api/v3/repos/{owner}/{name}");
    Mock::given(method("GET"))
        .and(path(repository_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 41,
            "name": name,
            "full_name": format!("{owner}/{name}"),
            "owner": { "login": owner },
            "default_branch": "main",
            "updated_at": "2026-09-20T12:00:00Z"
        })))
        .expect(expected_requests)
        .mount(server)
        .await;
}

/// Builds one static issue payload; pagination and acquisition remain visible in each test.
pub fn issue(id: u64, number: u64, title: &str) -> serde_json::Value {
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

/// Allocates a distinct on-disk archive path for concurrent integration cases.
pub fn temporary_archive_path() -> PathBuf {
    let next = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-thread-enumeration-{}-{next}.sqlite",
        std::process::id()
    ))
}

/// Removes a closed archive and SQLite sidecars after a successful scenario.
pub fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}
