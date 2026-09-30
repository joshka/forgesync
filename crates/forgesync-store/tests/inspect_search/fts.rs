//! # Full-text index cases
//!
//! These cases exercise the archive full-text search representation and update behavior. Search
//! documents are derived from discussion evidence, and index state must follow committed document
//! changes. Ranking policy belongs in the engine; this suite protects store retrieval.

use forgesync_core::content::{SourceState, ThreadKind};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_store::archive::Archive;

use crate::fixture::{
    discussion, keyword_query, remove_archive, repository, temporary_archive_path, thread_id,
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
    let content = discussion(
        &thread,
        ThreadKind::Issue,
        SourceState::Open,
        "Old title",
        Some("distinctive obsolete content"),
        "2026-09-20T10:00:00Z",
    );
    let observed_at = content.updated_at;
    let sequence = archive
        .reserve_observation_sequence(observed_at)
        .await
        .expect("reserve sequence");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        content,
        SourceClock::Valid(observed_at),
        observed_at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply thread observation");

    let original = archive
        .query_threads(&keyword_query("\"obsolete\""))
        .await
        .expect("query keyword page");
    assert_eq!(original.items.len(), 1);
    assert_eq!(original.items[0].discussion.id, thread);
    let content = discussion(
        &thread,
        ThreadKind::Issue,
        SourceState::Open,
        "Replacement title",
        None,
        "2026-09-20T10:00:01Z",
    );
    let observed_at = content.updated_at;
    let sequence = archive
        .reserve_observation_sequence(observed_at)
        .await
        .expect("reserve sequence");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        content,
        SourceClock::Valid(observed_at),
        observed_at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply thread observation");
    assert!(
        archive
            .query_threads(&keyword_query("\"obsolete\""))
            .await
            .expect("query keyword page")
            .items
            .is_empty()
    );
    let replacement = archive
        .query_threads(&keyword_query("\"replacement\""))
        .await
        .expect("query keyword page");
    assert_eq!(replacement.items.len(), 1);
    assert_eq!(replacement.items[0].discussion.id, thread);
    assert_eq!(replacement.items[0].discussion.title, "Replacement title");

    archive.close().await;
    remove_archive(&path);
}
