//! # Retrieval document content identity
//!
//! These cases establish which inputs change a document hash and which metadata stays outside it.
//! Provider timestamp changes preserve identity; recipe and rendered-text changes invalidate it.
//! The fixture fixes one source discussion and title so each scenario changes one selected input.
//!
//! Text is supplied directly to the core constructor. No engine recipe rendering, archive writes,
//! or embedding service calls are hidden in setup. Engine/store suites separately validate rendered
//! evidence and persisted freshness. These tests protect the core deterministic hash boundary with
//! direct comparisons rather than a workflow fixture.

use rstest::rstest;

use crate::document::{Document, DocumentRecipe};
use crate::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use crate::timestamp::UtcTimestamp;

/// Constructs a fixed discussion identity and title with scenario-selected recipe, time, and
/// text.
///
/// It renders no recipe sections; supplied text is the direct hash input under examination.
fn document(recipe: DocumentRecipe, source_updated_at: &str, text: &str) -> Document {
    let repository = RepositoryId::new(
        GitHubHost::parse("github.com").expect("host"),
        ProviderId::new("41").expect("repository ID"),
    );
    Document::new(
        ThreadId::new(
            repository,
            ProviderId::new("9001").expect("thread ID"),
            ThreadNumber::new(12).expect("thread number"),
        ),
        recipe,
        "Title".to_owned(),
        text.to_owned(),
        text.to_lowercase(),
        UtcTimestamp::parse(source_updated_at).expect("source timestamp"),
    )
}

#[test]
fn content_identity_ignores_provider_timestamp_changes() {
    let first = document(
        DocumentRecipe::OriginalBody,
        "2026-09-20T09:30:00Z",
        "# Title\n\nBody",
    );
    let later = document(
        DocumentRecipe::OriginalBody,
        "2026-09-21T09:30:00Z",
        "# Title\n\nBody",
    );

    assert_eq!(first.content_hash, later.content_hash);
    assert_eq!(first.content_hash.len(), 64);
    assert_eq!(first.content_hash, first.expected_content_hash());
}

#[rstest]
#[case::recipe(DocumentRecipe::DiscussionEnriched, "# Title\n\nBody")]
#[case::text(DocumentRecipe::OriginalBody, "# Title\n\nEdited body")]
fn changed_retrieval_inputs_invalidate_content_identity(
    #[case] recipe: DocumentRecipe,
    #[case] text: &str,
) {
    let original = document(
        DocumentRecipe::OriginalBody,
        "2026-09-20T09:30:00Z",
        "# Title\n\nBody",
    );
    let changed = document(recipe, "2026-09-20T09:30:00Z", text);

    assert_ne!(original.content_hash, changed.content_hash);
}
