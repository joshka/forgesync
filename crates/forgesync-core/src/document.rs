use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ThreadId, UtcTimestamp};

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
    /// Creates a versioned document and computes its stable SHA-256 identity.
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

    /// Recomputes the stored content hash to validate a document crossing a trust boundary.
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

fn add_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(value);
}

#[cfg(test)]
mod tests {
    use crate::{
        Document, DocumentRecipe, GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber,
        UtcTimestamp,
    };

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
    fn content_identity_ignores_source_retrieval_time() {
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

    #[test]
    fn recipe_and_relevant_text_changes_invalidate_content_identity() {
        let original = document(
            DocumentRecipe::OriginalBody,
            "2026-09-20T09:30:00Z",
            "# Title\n\nBody",
        );
        let enriched = document(
            DocumentRecipe::DiscussionEnriched,
            "2026-09-20T09:30:00Z",
            "# Title\n\nBody",
        );
        let changed = document(
            DocumentRecipe::OriginalBody,
            "2026-09-20T09:30:00Z",
            "# Title\n\nEdited body",
        );

        assert_ne!(original.content_hash, enriched.content_hash);
        assert_ne!(original.content_hash, changed.content_hash);
    }
}
