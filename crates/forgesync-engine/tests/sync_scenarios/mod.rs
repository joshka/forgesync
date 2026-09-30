//! # Focused sync scenarios
//!
//! This test module divides the engine sync workflow by evidence family and follow-on stage.
//! Enumeration, comments, reviews, documents, embeddings, refresh, and retry each have their own
//! fixture path. Read the relevant child before changing a workflow; the top-level sync suite
//! covers the full orchestration.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use forgesync_core::content::{Comment, Review, ReviewThread};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::GitHubHost;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_github::transport::{GitHubClient, GitHubClientConfig};
use forgesync_store::archive::Archive;
use forgesync_store::reads::ThreadQuery;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_string_contains, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

mod comments;
mod documents;
mod embeddings;
mod enumeration;
mod refresh;
mod retry;
mod reviews;

async fn sync_once(
    archive: &Archive,
    server: &MockServer,
    selector: RepositorySelector,
    scope: SyncThreadScope,
) -> forgesync_engine::sync::SyncReport {
    sync_once_with_comments(archive, server, selector, scope, false).await
}

async fn sync_once_with_comments(
    archive: &Archive,
    server: &MockServer,
    selector: RepositorySelector,
    scope: SyncThreadScope,
    include_comments: bool,
) -> forgesync_engine::sync::SyncReport {
    sync_once_with_families(
        archive,
        server,
        selector,
        scope,
        include_comments,
        false,
        false,
    )
    .await
}

async fn sync_once_with_families(
    archive: &Archive,
    server: &MockServer,
    selector: RepositorySelector,
    scope: SyncThreadScope,
    include_comments: bool,
    include_reviews: bool,
    include_review_threads: bool,
) -> forgesync_engine::sync::SyncReport {
    let clients = clients_for(server, &selector);
    sync_repositories(
        archive,
        &clients,
        &SyncRequest {
            repositories: vec![selector],
            all: false,
            scope,
            include_comments,
            include_reviews,
            include_review_threads,
            parent_run: None,
        },
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report")
}

fn clients_for(
    server: &MockServer,
    selector: &RepositorySelector,
) -> HashMap<GitHubHost, GitHubClient> {
    let api_base_url = format!("{}/api/v3/", server.uri())
        .parse()
        .expect("local API URL");
    let client =
        GitHubClient::new(GitHubClientConfig::new(api_base_url), None).expect("GitHub client");
    HashMap::from([(selector.host().clone(), client)])
}

async fn mount_open_issues(server: &MockServer, issues: Vec<serde_json::Value>) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "open"))
        .respond_with(ResponseTemplate::new(200).set_body_json(issues))
        .mount(server)
        .await;
}

async fn mount_document_source(
    server: &MockServer,
    issue_updated_at: &str,
    comment_updated_at: &str,
    comment_body: &str,
) {
    mount_repository(server).await;
    let mut issue = issue_with_comment_count(91, 11, "Document target", issue_updated_at, 1);
    issue["body"] = json!("Original discussion body");
    mount_open_issues(server, vec![issue]).await;
    let mut comment = comment(1101, comment_body);
    comment["updated_at"] = json!(comment_updated_at);
    mount_comments(server, 11, vec![comment]).await;
}

fn pull_request_issue(updated_at: &str) -> serde_json::Value {
    let mut issue = issue(1802, 18, "selected change");
    issue["updated_at"] = json!(updated_at);
    issue["comments"] = json!(1);
    issue["html_url"] = json!("https://github.com/owner/repo/pull/18");
    issue["pull_request"] = json!({
        "url": "https://api.github.com/repos/owner/repo/pulls/18"
    });
    issue
}

async fn mount_pull_request_metadata(server: &MockServer, head_sha: &str, merged: bool) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/18"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "base": {
                "ref": "main",
                "sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "repo": { "id": 41, "full_name": "owner/repo" }
            },
            "head": {
                "ref": "topic",
                "sha": head_sha,
                "repo": { "id": 41, "full_name": "owner/repo" }
            },
            "draft": false,
            "merged": merged
        })))
        .mount(server)
        .await;
}

async fn mount_pull_reviews(server: &MockServer, status: u16, reviews: Vec<serde_json::Value>) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/18/reviews"))
        .and(query_param("per_page", "100"))
        .respond_with(ResponseTemplate::new(status).set_body_json(reviews))
        .mount(server)
        .await;
}

async fn mount_graphql_review_threads(server: &MockServer, response: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("reviewThreads(first: 100"))
        .and(body_string_contains("\"cursor\":null"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .mount(server)
        .await;
}

fn review_thread(id: &str, is_resolved: bool) -> serde_json::Value {
    json!({
        "id": id,
        "isResolved": is_resolved,
        "isOutdated": false,
        "path": "src/lib.rs",
        "line": 42,
        "startLine": null,
        "viewerCanResolve": true,
        "comments": {
            "nodes": [],
            "pageInfo": {"hasNextPage": false, "endCursor": null}
        }
    })
}

fn review_thread_page(
    review_threads: Vec<serde_json::Value>,
    has_next_page: bool,
    end_cursor: Option<&str>,
) -> serde_json::Value {
    json!({
        "data": {"repository": {"pullRequest": {"reviewThreads": {
            "nodes": review_threads,
            "pageInfo": {"hasNextPage": has_next_page, "endCursor": end_cursor}
        }}}}
    })
}

fn pull_review(id: u64, commit_sha: &str) -> serde_json::Value {
    json!({
        "id": id,
        "state": "CHANGES_REQUESTED",
        "body": "Please revise this change.",
        "submitted_at": "2026-09-19T12:00:00Z",
        "commit_id": commit_sha,
        "user": { "id": 51, "login": "reviewer", "type": "User" },
        "author_association": "MEMBER"
    })
}

async fn mount_comments(server: &MockServer, number: u64, comments: Vec<serde_json::Value>) {
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v3/repos/owner/repo/issues/{number}/comments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(comments))
        .mount(server)
        .await;
}

async fn thread_summary(archive: &Archive, number: u64) -> forgesync_store::reads::ThreadSummary {
    archive
        .query_threads(&ThreadQuery {
            repositories: Vec::new(),
            kind: None,
            state: forgesync_store::reads::ThreadStateFilter::All,
            match_expression: None,
            updated_since: None,
            sort: forgesync_store::reads::ThreadSort::Updated,
            limit: NonZeroU32::new(1000).expect("positive limit"),
            offset: 0,
        })
        .await
        .expect("query all threads")
        .items
        .into_iter()
        .find(|thread| thread.discussion.id.number().get() == number)
        .expect("thread summary")
}

fn comment_coverage(
    summary: &forgesync_store::reads::ThreadSummary,
) -> &forgesync_core::coverage::Coverage {
    summary
        .coverage
        .iter()
        .find(|coverage| coverage.family() == EvidenceFamily::Comments)
        .expect("comment coverage")
}

fn review_coverage(
    summary: &forgesync_store::reads::ThreadSummary,
) -> &forgesync_core::coverage::Coverage {
    summary
        .coverage
        .iter()
        .find(|coverage| coverage.family() == EvidenceFamily::Reviews)
        .expect("review coverage")
}

fn review_thread_coverage(
    summary: &forgesync_store::reads::ThreadSummary,
) -> &forgesync_core::coverage::Coverage {
    summary
        .coverage
        .iter()
        .find(|coverage| coverage.family() == EvidenceFamily::ReviewThreads)
        .expect("review-thread coverage")
}

async fn comment_bodies(archive: &Archive, number: u64) -> Vec<String> {
    let summary = thread_summary(archive, number).await;
    archive
        .child_family_members::<Comment>(&summary.discussion.id, EvidenceFamily::Comments)
        .await
        .expect("read canonical comments")
        .into_iter()
        .map(|item| item.payload.body)
        .collect()
}

async fn review_members(
    archive: &Archive,
    number: u64,
) -> Vec<forgesync_store::observations::StagedItem<Review>> {
    let summary = thread_summary(archive, number).await;
    archive
        .child_family_members::<Review>(&summary.discussion.id, EvidenceFamily::Reviews)
        .await
        .expect("read canonical reviews")
}

async fn review_thread_members(
    archive: &Archive,
    number: u64,
) -> Vec<forgesync_store::observations::StagedItem<ReviewThread>> {
    let summary = thread_summary(archive, number).await;
    archive
        .child_family_members::<ReviewThread>(&summary.discussion.id, EvidenceFamily::ReviewThreads)
        .await
        .expect("read canonical review threads")
}

async fn mount_repository(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 41,
            "name": "repo",
            "full_name": "owner/repo",
            "owner": { "login": "owner" },
            "default_branch": "main",
            "updated_at": "2026-09-20T12:00:00Z"
        })))
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
        "comments": 0,
        "closed_at": null,
        "html_url": format!("https://github.com/owner/repo/issues/{number}"),
        "labels": [],
        "assignees": [],
        "user": { "login": "maintainer" }
    })
}

fn issue_with_comment_count(
    id: u64,
    number: u64,
    title: &str,
    updated_at: &str,
    comments: u64,
) -> serde_json::Value {
    let mut issue = issue(id, number, title);
    issue["updated_at"] = json!(updated_at);
    issue["comments"] = json!(comments);
    issue
}

fn comment(id: u64, body: &str) -> serde_json::Value {
    json!({
        "id": id,
        "body": body,
        "created_at": "2026-09-19T08:00:00Z",
        "updated_at": "2026-09-19T08:00:00Z",
        "user": { "login": "reviewer" }
    })
}

async fn thread_count(archive: &Archive) -> usize {
    archive
        .query_threads(&ThreadQuery {
            repositories: Vec::new(),
            kind: None,
            state: forgesync_store::reads::ThreadStateFilter::All,
            match_expression: None,
            updated_since: None,
            sort: forgesync_store::reads::ThreadSort::Updated,
            limit: NonZeroU32::new(20).expect("positive limit"),
            offset: 0,
        })
        .await
        .expect("query threads")
        .items
        .len()
}

fn temporary_archive_path() -> PathBuf {
    let next = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-sync-workflow-{}-{next}.sqlite",
        std::process::id()
    ))
}

fn current_timestamp() -> UtcTimestamp {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after Unix epoch");
    UtcTimestamp::from_unix_microseconds(
        i64::try_from(elapsed.as_micros()).expect("current timestamp fits"),
    )
    .expect("valid current timestamp")
}

fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}
