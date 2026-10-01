//! Which inputs change a document hash; a fixed digest protects field order and framing.

use rstest::rstest;

use crate::document::{Document, DocumentRecipe};
use crate::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use crate::timestamp::UtcTimestamp;

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

#[test]
fn original_body_hash_retains_version_one_field_encoding() {
    let value = document(
        DocumentRecipe::OriginalBody,
        "2026-09-20T09:30:00Z",
        "# Title\n\nBody",
    );

    assert_eq!(
        value.content_hash,
        "54e41618fb22e9b5230d71ac0b25976110f1ec68436a0787edf2d457b7d5c00e"
    );
    assert_eq!(value.expected_content_hash(), value.content_hash);
}

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
