//! # Document materialization scenarios
//!
//! These cases build derived search documents from archived discussion evidence. They cover the
//! boundary between source observations and recipe-shaped text. A recipe or observation change may
//! require regeneration, while source content remains stored independently.

use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::documents::{build_thread_document, materialize_thread_document};
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;
use wiremock::MockServer;

use super::{clients_for, mount_document_source, remove_archive, temporary_archive_path};

#[tokio::test]
async fn document_materialization_tracks_content_but_ignores_source_timestamps() {
    let server = MockServer::start().await;
    let archive_path = temporary_archive_path();
    let archive = Archive::create(&archive_path)
        .await
        .expect("create archive");
    let repository = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("repository selector");
    let reference = "owner/repo#11"
        .parse::<ThreadSelector>()
        .expect("thread selector");

    mount_document_source(
        &server,
        "2026-09-20T09:30:00Z",
        "2026-09-19T08:00:00Z",
        "Stable discussion reply",
    )
    .await;
    let clients = clients_for(&server, &repository);
    let request = SyncRequest {
        repositories: vec![repository.clone()],
        all: false,
        scope: SyncThreadScope::Open,
        include_comments: true,
        include_reviews: false,
        include_review_threads: false,
        parent_run: None,
    };
    let initial_sync = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert_eq!(initial_sync.outcome, OperationOutcome::Complete);

    let built = build_thread_document(
        &archive,
        &reference,
        forgesync_core::document::DocumentRecipe::DiscussionEnriched,
    )
    .await
    .expect("build document");
    let first = materialize_thread_document(
        &archive,
        &reference,
        forgesync_core::document::DocumentRecipe::DiscussionEnriched,
    )
    .await
    .expect("materialize first document");
    assert_eq!(first.document, built);
    assert!(first.write.content_changed);
    let stored_first = archive
        .document(&first.document.source_identity, first.document.recipe)
        .await
        .expect("load first document")
        .expect("document is stored");
    assert_eq!(stored_first, first.document);

    let repeated = materialize_thread_document(
        &archive,
        &reference,
        forgesync_core::document::DocumentRecipe::DiscussionEnriched,
    )
    .await
    .expect("materialize identical document");
    assert!(!repeated.write.content_changed);
    assert_eq!(repeated.write.id, first.write.id);
    assert_eq!(repeated.document.content_hash, first.document.content_hash);

    server.reset().await;
    mount_document_source(
        &server,
        "2026-09-20T09:31:00Z",
        "2026-09-19T08:00:00Z",
        "Stable discussion reply",
    )
    .await;
    let clients = clients_for(&server, &repository);
    let request = SyncRequest {
        repositories: vec![repository.clone()],
        all: false,
        scope: SyncThreadScope::Open,
        include_comments: true,
        include_reviews: false,
        include_review_threads: false,
        parent_run: None,
    };
    let timestamp_sync = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert_eq!(timestamp_sync.outcome, OperationOutcome::Complete);
    let timestamp_only = materialize_thread_document(
        &archive,
        &reference,
        forgesync_core::document::DocumentRecipe::DiscussionEnriched,
    )
    .await
    .expect("materialize timestamp-only change");
    assert!(!timestamp_only.write.content_changed);
    assert_eq!(
        timestamp_only.document.content_hash,
        first.document.content_hash
    );
    assert_ne!(
        timestamp_only.document.source_updated_at,
        first.document.source_updated_at
    );

    server.reset().await;
    mount_document_source(
        &server,
        "2026-09-20T09:32:00Z",
        "2026-09-20T09:32:00Z",
        "Edited discussion reply",
    )
    .await;
    let clients = clients_for(&server, &repository);
    let request = SyncRequest {
        repositories: vec![repository],
        all: false,
        scope: SyncThreadScope::Open,
        include_comments: true,
        include_reviews: false,
        include_review_threads: false,
        parent_run: None,
    };
    let edited_sync = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert_eq!(edited_sync.outcome, OperationOutcome::Complete);
    let edited = materialize_thread_document(
        &archive,
        &reference,
        forgesync_core::document::DocumentRecipe::DiscussionEnriched,
    )
    .await
    .expect("materialize edited document");
    assert!(edited.write.content_changed);
    assert_ne!(edited.document.content_hash, first.document.content_hash);
    assert!(edited.document.text.contains("Edited discussion reply"));

    archive.close().await;
    remove_archive(&archive_path);
}
