//! Refresh workflow contracts.

use super::{
    Archive, CancellationToken, DocumentRecipe, MockServer, OperationOutcome, RefreshAnalysisStage,
    RefreshRequest, RefreshStageKind, RefreshStageStatus, RefreshSyncOptions, RepositorySelector,
    SyncThreadScope, clients_for, mount_open_issues, mount_repository, refresh, remove_archive,
    temporary_archive_path,
};

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
    assert_eq!(
        report.sync.as_ref().unwrap().status,
        RefreshStageStatus::Complete
    );
    assert!(report.sync.as_ref().unwrap().report.is_some());
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
        analysis: vec![RefreshAnalysisStage::Embeddings],
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
    assert!(sync.report.is_some());
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
