//! # Full-text index cases
//!
//! This regression acquires a discussion containing a distinctive body term, proves that it can be
//! retrieved, then publishes newer parent evidence with a replacement title and no body. The old
//! term must disappear while the new title resolves to the same discussion identity.
//!
//! Both acquisitions reserve their own sequence and apply their complete observation visibly.
//! Keyword request construction is pure; queries and their failure boundaries stay in the scenario.
//! The dependent before/after steps remain together because a replacement assertion needs proof
//! that the original indexed term existed.
//!
//! This checks the committed FTS representation after parent replacement. Transactional rollback
//! has dedicated observation regressions; no injected write failure occurs here. Engine relevance
//! policy, provider acquisition, and schema backfill have separate owners.

use forgesync_core::content::{SourceState, ThreadKind};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_store::archive::Archive;

use crate::fixture::{
    discussion, keyword_query, remove_archive, repository, temporary_archive_path, thread_id,
};

#[tokio::test]
async fn fts_index_replaces_removed_body_with_current_title() {
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
        .apply_thread_observation(&observation, None)
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
        .apply_thread_observation(&observation, None)
        .await
        .expect("apply thread observation");
    let obsolete = archive
        .query_threads(&keyword_query("\"obsolete\""))
        .await
        .expect("query removed body term");
    assert!(obsolete.items.is_empty());
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
