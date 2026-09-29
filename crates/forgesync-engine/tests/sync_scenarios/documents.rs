//! Documents workflow contracts.

use super::{
    Archive, MockServer, OperationOutcome, RepositorySelector, SyncThreadScope, ThreadSelector,
    build_thread_document, materialize_thread_document, mount_document_source, remove_archive,
    sync_once_with_comments, temporary_archive_path,
};

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
    let initial_sync = sync_once_with_comments(
        &archive,
        &server,
        repository.clone(),
        SyncThreadScope::Open,
        true,
    )
    .await;
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
    let timestamp_sync = sync_once_with_comments(
        &archive,
        &server,
        repository.clone(),
        SyncThreadScope::Open,
        true,
    )
    .await;
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
    let edited_sync =
        sync_once_with_comments(&archive, &server, repository, SyncThreadScope::Open, true).await;
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
