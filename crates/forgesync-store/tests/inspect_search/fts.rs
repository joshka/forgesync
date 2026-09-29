use super::*;

#[tokio::test]
async fn fts_index_tracks_updates_and_removed_text_transactionally() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository("example", "search", "repo-search");
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let thread = thread_id(&repository.id, "thread-search", 1);
    apply_thread(
        &archive,
        discussion(
            &thread,
            ThreadKind::Issue,
            SourceState::Open,
            "Old title",
            Some("distinctive obsolete content"),
            "2026-09-20T10:00:00Z",
        ),
    )
    .await;

    assert_eq!(query(&archive, "\"obsolete\"").await.items.len(), 1);
    apply_thread(
        &archive,
        discussion(
            &thread,
            ThreadKind::Issue,
            SourceState::Open,
            "Replacement title",
            None,
            "2026-09-20T10:00:01Z",
        ),
    )
    .await;
    assert!(query(&archive, "\"obsolete\"").await.items.is_empty());
    assert_eq!(query(&archive, "\"replacement\"").await.items.len(), 1);

    archive.close().await;
    remove_archive(&path);
}
