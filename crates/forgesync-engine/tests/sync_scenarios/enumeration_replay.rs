//! # Interrupted page replay retains committed discussions
//!
//! A delayed second page gives cancellation a deterministic point after the first page commits.
//! Its responder notifies the scenario when that request arrives; no polling loop or
//! request-history search is needed to discover the boundary. The task returns its archive for
//! direct inspection.
//!
//! The interrupted report records one completed page and one pending job. A local detail read
//! proves the first discussion exists before a new sync replays the pages. The completed run must
//! preserve that entire discussion payload and add the second identity without duplicate rows.
//!
//! Requests and engine operations remain explicit. The initial and resumed phases stay together
//! because replay preservation needs its committed baseline; checkpoint overlap and writer cleanup
//! have independent sibling scenarios.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_github::transport::{GitHubClient, GitHubClientConfig};
use forgesync_store::archive::Archive;
use forgesync_store::runs::SyncJobStatus;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fixture_archive::{remove_archive, temporary_archive_path, thread_reference};
use super::fixture_issues::{clients_for, issue, mount_repository};

#[tokio::test]
async fn interrupted_page_replay_keeps_committed_threads_without_duplicates() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "open"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", "<?page=2>; rel=\"next\"")
                .set_body_json(json!([issue(91, 11, "page one")])),
        )
        .mount(&server)
        .await;
    let second_page_started = Arc::new(tokio::sync::Notify::new());
    let page_signal = Arc::clone(&second_page_started);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("page", "2"))
        .respond_with(move |_: &wiremock::Request| {
            page_signal.notify_one();
            ResponseTemplate::new(200).set_delay(Duration::from_secs(30))
        })
        .mount(&server)
        .await;

    let archive_path = temporary_archive_path();
    let archive = Archive::create(&archive_path)
        .await
        .expect("create archive");
    let selector = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("selector");
    let cancellation = CancellationToken::new();
    let task_cancellation = cancellation.clone();
    let host = selector.host().clone();
    let api_base_url = format!("{}/api/v3/", server.uri())
        .parse()
        .expect("local API URL");
    let client =
        GitHubClient::new(GitHubClientConfig::new(api_base_url), None).expect("GitHub client");
    let clients = HashMap::from([(host, client)]);
    let request = SyncRequest {
        repositories: vec![selector.clone()],
        all: false,
        scope: SyncThreadScope::Open,
        include_comments: false,
        include_reviews: false,
        include_review_threads: false,
        parent_run: None,
    };
    let sync_task = tokio::spawn(async move {
        let result =
            sync_repositories(&archive, &clients, &request, &task_cancellation, None).await;
        (archive, result)
    });
    tokio::time::timeout(Duration::from_secs(5), second_page_started.notified())
        .await
        .expect("second page request starts");
    cancellation.cancel();
    let (archive, first_result) = sync_task.await.expect("sync task");
    let first = first_result.expect("interrupted sync report");
    assert_eq!(
        first.outcome,
        OperationOutcome::Interrupted { pending_items: 1 }
    );
    assert_eq!(first.jobs.len(), 1);
    assert_eq!(first.jobs[0].status, SyncJobStatus::Interrupted);
    assert_eq!(first.pages_completed, 1);
    assert_eq!(first.threads_seen, 1);
    let status = archive.archive_status().await.expect("read archive counts");
    assert_eq!(status.threads, 1);
    let retained = archive
        .thread_detail(&thread_reference(11))
        .await
        .expect("read committed page-one discussion");
    assert_eq!(retained.summary.discussion.title, "page one");

    server.reset().await;
    mount_repository(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "open"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", "<?page=2>; rel=\"next\"")
                .set_body_json(json!([issue(91, 11, "page one")])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([issue(92, 12, "page two")])))
        .mount(&server)
        .await;

    let clients = clients_for(&server, &selector);
    let request = SyncRequest {
        repositories: vec![selector],
        all: false,
        scope: SyncThreadScope::Open,
        include_comments: false,
        include_reviews: false,
        include_review_threads: false,
        parent_run: None,
    };
    let resumed = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert_eq!(resumed.outcome, OperationOutcome::Complete);
    assert_eq!(resumed.pages_completed, 2);
    assert_eq!(resumed.threads_seen, 2);
    let status = archive.archive_status().await.expect("read archive counts");
    assert_eq!(status.threads, 2);
    let replayed = archive
        .thread_detail(&thread_reference(11))
        .await
        .expect("read replayed page-one discussion");
    let added = archive
        .thread_detail(&thread_reference(12))
        .await
        .expect("read completed page-two discussion");
    assert_eq!(replayed.summary.discussion, retained.summary.discussion);
    assert_eq!(added.summary.discussion.title, "page two");

    archive.close().await;
    remove_archive(&archive_path);
}
