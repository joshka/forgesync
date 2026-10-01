//! # Invalid full-text syntax rejection
//!
//! An archive with indexed content exercises malformed FTS query syntax.
//! The expression has an unclosed proximity group and must produce the typed search-query error.
//! Query construction fixes incidental scope and paging settings without executing the query.
//! One complete thread observation supplies the indexed row needed to evaluate the expression.
//!
//! The public query operation remains visible beside its failure assertion.
//! This case establishes syntax rejection rather than ranking or empty-result behavior.
//! Accepted expressions and scope coverage live in their own scenarios.
//! Cleanup follows explicit archive closure.

use forgesync_core::content::{SourceState, ThreadKind};
use forgesync_core::observation::ThreadObservation;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;

use crate::fixture::{
    discussion, keyword_query, remove_archive, repository, temporary_archive_path, thread_id,
};

#[tokio::test]
async fn malformed_search_expression_has_a_typed_error() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");

    let repository = repository("example", "first", "repo-first");
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let first_thread = thread_id(&repository.id, "thread-1", 1);
    let content = discussion(
        &first_thread,
        ThreadKind::Issue,
        SourceState::Open,
        "Needle in title",
        Some("body text"),
        "2026-09-20T10:00:00Z",
    );
    let observed_at = content.updated_at;
    let sequence = archive
        .reserve_observation_sequence(observed_at)
        .await
        .expect("reserve sequence");
    let observation = ThreadObservation {
        discussion: content,
        observed_at,
        sequence,
    };
    archive
        .apply_thread_observation(&observation, None)
        .await
        .expect("apply thread observation");

    let result = archive.query_threads(&keyword_query("NEAR(")).await;

    assert!(matches!(result, Err(StoreError::InvalidSearchQuery)));
    archive.close().await;
    remove_archive(&path);
}
