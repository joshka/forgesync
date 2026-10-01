//! # Refresh stage scenarios
//!
//! These two cases distinguish sync-only refresh from refresh that also requests embeddings without
//! a configured service. Both acquire the same empty repository through a real engine operation;
//! model-service construction is unnecessary for sync-only work.
//!
//! The sync-only result selects exactly sync and has no embedding/cluster stages or pending work.
//! When embedding is requested but unavailable, the complete sync report and its durable run
//! survive, while embedding reports a specific failure and remains pending for later work.
//!
//! Requests name optional stages and service identity explicitly. Setup configures provider
//! fixtures without invoking refresh; assertions distinguish selected, completed, failed, and
//! remaining work. Stage sequencing is the shared policy owned by this small suite, so the two
//! cases stay together.

use forgesync_core::document::DocumentRecipe;
use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::refresh::{
    RefreshRequest, RefreshStageKind, RefreshStageStatus, RefreshSyncOptions, refresh,
};
use forgesync_engine::sync::SyncThreadScope;
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;
use wiremock::MockServer;

use super::fixture_archive::{remove_archive, temporary_archive_path};
use super::fixture_issues::{clients_for, mount_open_issues, mount_repository};

#[tokio::test]
async fn refresh_syncs_without_constructing_a_model_service() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    mount_open_issues(&server, Vec::new()).await;

    let archive_path = temporary_archive_path();
    let archive = Archive::create(&archive_path)
        .await
        .expect("create archive");
    let selector = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("selector");
    let clients = clients_for(&server, &selector);
    let request = RefreshRequest {
        repositories: vec![selector],
        sync: Some(RefreshSyncOptions {
            scope: SyncThreadScope::Open,
            include_comments: false,
            include_reviews: false,
            include_review_threads: false,
        }),
        analysis: Vec::new(),
        recipe: DocumentRecipe::OriginalBody,
        embedding_identity: None,
        force_embeddings: false,
        cluster_options: forgesync_engine::clustering::ClusterOptions::default(),
    };

    let report = refresh(
        &archive,
        &clients,
        None,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("refresh report");

    assert_eq!(report.selected, [RefreshStageKind::Sync]);
    let sync = report.sync.as_ref().expect("sync stage reported");
    assert_eq!(sync.status, RefreshStageStatus::Complete);
    let sync_report = sync
        .report
        .as_ref()
        .expect("completed sync report retained");
    assert_eq!(sync_report.outcome, OperationOutcome::Complete);
    assert!(report.embeddings.is_none());
    assert!(report.clusters.is_none());
    assert!(report.remaining.is_empty());
    assert_eq!(report.outcome, OperationOutcome::Complete);

    archive.close().await;
    remove_archive(&archive_path);
}

#[tokio::test]
async fn refresh_retains_sync_when_an_optional_embedding_stage_is_unavailable() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    mount_open_issues(&server, Vec::new()).await;

    let archive_path = temporary_archive_path();
    let archive = Archive::create(&archive_path)
        .await
        .expect("create archive");
    let selector = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("selector");
    let clients = clients_for(&server, &selector);
    let request = RefreshRequest {
        repositories: vec![selector],
        sync: Some(RefreshSyncOptions {
            scope: SyncThreadScope::Open,
            include_comments: false,
            include_reviews: false,
            include_review_threads: false,
        }),
        analysis: vec![RefreshStageKind::Embeddings],
        recipe: DocumentRecipe::OriginalBody,
        embedding_identity: None,
        force_embeddings: false,
        cluster_options: forgesync_engine::clustering::ClusterOptions::default(),
    };

    let report = refresh(
        &archive,
        &clients,
        None,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("partial refresh report");

    let sync = report.sync.as_ref().expect("sync stage retained");
    assert_eq!(sync.status, RefreshStageStatus::Complete);
    let sync_report = sync
        .report
        .as_ref()
        .expect("completed sync report retained");
    assert_eq!(sync_report.outcome, OperationOutcome::Complete);
    let persisted = archive
        .run_detail(sync_report.run.id)
        .await
        .expect("read retained sync run");
    let persisted = persisted.expect("sync run remains durable");
    assert_eq!(persisted.run.outcome, Some(OperationOutcome::Complete));
    let embeddings = report
        .embeddings
        .as_ref()
        .expect("embedding stage reported");
    assert_eq!(embeddings.status, RefreshStageStatus::Failed);
    assert_eq!(
        embeddings.failure.as_ref().map(|failure| failure.code),
        Some("embedding_service_unavailable")
    );
    assert_eq!(report.remaining, [RefreshStageKind::Embeddings]);
    assert_eq!(
        report.outcome,
        OperationOutcome::Partial {
            failed_items: 1,
            deferred_items: 0,
        }
    );

    archive.close().await;
    remove_archive(&archive_path);
}
