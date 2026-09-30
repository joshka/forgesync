//! # Full-text index cases
//!
//! These cases exercise the archive full-text search representation and update behavior. Search
//! documents are derived from discussion evidence, and index state must follow committed document
//! changes. Ranking policy belongs in the engine; this suite protects store retrieval.

use forgesync_core::content::{SourceState, ThreadKind};
use forgesync_store::archive::Archive;

use super::{
    apply_thread, discussion, keyword_page, remove_archive, repository, temporary_archive_path,
    thread_id,
};

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

    let original = keyword_page(&archive, "\"obsolete\"").await;
    assert_eq!(original.items.len(), 1);
    assert_eq!(original.items[0].discussion.id, thread);
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
    assert!(
        keyword_page(&archive, "\"obsolete\"")
            .await
            .items
            .is_empty()
    );
    let replacement = keyword_page(&archive, "\"replacement\"").await;
    assert_eq!(replacement.items.len(), 1);
    assert_eq!(replacement.items[0].discussion.id, thread);
    assert_eq!(replacement.items[0].discussion.title, "Replacement title");

    archive.close().await;
    remove_archive(&path);
}
