//! Versioned text derived from a discussion for local retrieval.
//!
//! [`DocumentRecipe`] selects which normalized evidence contributes to searchable text.
//! [`Document`] keeps the resulting text, content hash, and source context together. The engine
//! constructs it from a stored thread detail; the store persists it separately from the underlying
//! discussion.
//!
//! A recipe version matters because a text-construction change can make old stored embeddings
//! stale even when the provider discussion has not changed. A document is derived state, not a
//! replacement for a discussion observation. Embedding compatibility also depends on the model
//! identity and chunk rules in [`crate::embedding`].
//!
//! Use the recipe when materializing or selecting retrieval inputs. Keep rules for which sections
//! enter the text in `forgesync-engine::documents`; this module defines the value and its
//! versioned identity rather than fetching child resources.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::identity::ThreadId;
use crate::timestamp::UtcTimestamp;

/// Recipe selected to turn normalized discussion evidence into a retrieval document.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentRecipe {
    /// Use the source title, body, and labels only.
    OriginalBody,
    /// Add current comments and selected pull-request review evidence.
    #[default]
    DiscussionEnriched,
}

impl DocumentRecipe {
    /// Current version for deterministic text produced by this recipe.
    pub const VERSION: u32 = 1;

    /// Stable storage and configuration name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OriginalBody => "original_body",
            Self::DiscussionEnriched => "discussion_enriched",
        }
    }
}

/// Deterministic retrieval input with the identity and recipe that produced it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Stable source discussion identity, including its host-qualified repository identity.
    pub source_identity: ThreadId,
    /// Recipe used to select source fields.
    pub recipe: DocumentRecipe,
    /// Version of the recipe's text contract.
    pub recipe_version: u32,
    /// SHA-256 over recipe, version, source identity, title, and document text.
    pub content_hash: String,
    /// Normalized source title for display and filtering.
    pub title: String,
    /// Complete deterministic input text.
    pub text: String,
    /// Whitespace-normalized, lower-case text for deduplication and similarity checks.
    pub dedupe_text: String,
    /// Latest parent source timestamp; excluded from content hashing.
    pub source_updated_at: UtcTimestamp,
}

impl Document {
    /// Packages already rendered retrieval text with the current recipe version and content hash.
    ///
    /// The engine owns recipe rendering. This constructor preserves `title`, `text`, and
    /// `dedupe_text` as supplied; it does not normalize whitespace, lowercase deduplication text,
    /// fetch source evidence, or verify that rendered sections agree with the chosen recipe.
    ///
    /// The hash covers source identity, recipe/version, title, and text. Deduplication text and the
    /// source timestamp are deliberately excluded: a source-clock-only change leaves retrieval
    /// identity stable. Because fields are public, a value received or modified after construction
    /// still needs store-boundary validation before persistence.
    pub fn new(
        source_identity: ThreadId,
        recipe: DocumentRecipe,
        title: String,
        text: String,
        dedupe_text: String,
        source_updated_at: UtcTimestamp,
    ) -> Self {
        let recipe_version = DocumentRecipe::VERSION;
        let content_hash = content_hash(&source_identity, recipe, recipe_version, &title, &text);
        Self {
            source_identity,
            recipe,
            recipe_version,
            content_hash,
            title,
            text,
            dedupe_text,
            source_updated_at,
        }
    }

    /// Recomputes the expected hash from the document's current identity, recipe, title, and text.
    ///
    /// Compare this result with [`Self::content_hash`] when validating an external or modified
    /// document. Calling this query changes no field and does not validate recipe support,
    /// deduplication normalization, or source-clock consistency. The store applies its broader
    /// validation rules separately.
    pub fn expected_content_hash(&self) -> String {
        content_hash(
            &self.source_identity,
            self.recipe,
            self.recipe_version,
            &self.title,
            &self.text,
        )
    }
}

/// Hashes the source identity, recipe, and rendered text; acquisition time is deliberately absent
/// so a repeated fetch of unchanged content keeps the same document identity.
fn content_hash(
    source_identity: &ThreadId,
    recipe: DocumentRecipe,
    recipe_version: u32,
    title: &str,
    text: &str,
) -> String {
    let mut hasher = Sha256::new();
    add_field(&mut hasher, b"forgesync-document-v1");
    add_field(&mut hasher, recipe.as_str().as_bytes());
    add_field(&mut hasher, &recipe_version.to_be_bytes());
    add_field(
        &mut hasher,
        source_identity.repository().host().as_str().as_bytes(),
    );
    add_field(
        &mut hasher,
        source_identity
            .repository()
            .provider_id()
            .as_str()
            .as_bytes(),
    );
    add_field(
        &mut hasher,
        source_identity.provider_id().as_str().as_bytes(),
    );
    add_field(&mut hasher, &source_identity.number().get().to_be_bytes());
    add_field(&mut hasher, title.as_bytes());
    add_field(&mut hasher, text.as_bytes());
    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

/// Prefixes each field with its length so adjacent fields cannot produce the same byte stream.
fn add_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(value);
}

#[cfg(test)]
mod tests {
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
}
