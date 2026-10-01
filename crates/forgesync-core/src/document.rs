//! Versioned text derived from a discussion for local retrieval.
//!
//! A recipe version matters because a text-construction change can make old stored embeddings
//! stale even when the provider discussion has not changed. The rules for which sections enter the
//! text live in `forgesync-engine::documents`.

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
    pub source_identity: ThreadId,
    pub recipe: DocumentRecipe,
    pub recipe_version: u32,
    /// SHA-256 over recipe, version, source identity, title, and document text.
    pub content_hash: String,
    pub title: String,
    pub text: String,
    /// Whitespace-normalized, lower-case text for deduplication and similarity checks.
    pub dedupe_text: String,
    /// Latest parent source timestamp; excluded from content hashing.
    pub source_updated_at: UtcTimestamp,
}

impl Document {
    /// Packages already rendered retrieval text with the current recipe version and content hash.
    ///
    /// Deduplication text and the source timestamp are excluded from the hash so a
    /// source-clock-only change leaves retrieval identity stable.
    pub fn new(
        source_identity: ThreadId,
        recipe: DocumentRecipe,
        title: String,
        text: String,
        dedupe_text: String,
        source_updated_at: UtcTimestamp,
    ) -> Self {
        let mut document = Self {
            source_identity,
            recipe,
            recipe_version: DocumentRecipe::VERSION,
            content_hash: String::new(),
            title,
            text,
            dedupe_text,
            source_updated_at,
        };
        document.content_hash = document.expected_content_hash();
        document
    }

    /// Recomputes the lowercase-hex SHA-256 over length-prefixed identity, recipe, title, and text.
    pub fn expected_content_hash(&self) -> String {
        let mut hasher = Sha256::new();
        add_field(&mut hasher, b"forgesync-document-v1");
        add_field(&mut hasher, self.recipe.as_str().as_bytes());
        add_field(&mut hasher, &self.recipe_version.to_be_bytes());
        add_field(
            &mut hasher,
            self.source_identity.repository().host().as_str().as_bytes(),
        );
        add_field(
            &mut hasher,
            self.source_identity
                .repository()
                .provider_id()
                .as_str()
                .as_bytes(),
        );
        add_field(
            &mut hasher,
            self.source_identity.provider_id().as_str().as_bytes(),
        );
        add_field(
            &mut hasher,
            &self.source_identity.number().get().to_be_bytes(),
        );
        add_field(&mut hasher, self.title.as_bytes());
        add_field(&mut hasher, self.text.as_bytes());
        let digest = hasher.finalize();
        let mut output = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(output, "{byte:02x}");
        }
        output
    }
}

/// Prefixes each field with its length so adjacent fields cannot produce the same byte stream.
fn add_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(value);
}

#[cfg(test)]
mod tests;
