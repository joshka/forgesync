//! Configuration parsing and service-boundary examples.
//!
//! These cases distinguish default selection, explicit document recipes, parse rejection, and
//! semantic embedding validation. They construct settings directly rather than mutating process
//! environment, keeping the expected input visible and safe under concurrent tests. Network and
//! credential resolution belong to their own boundaries; successful validation makes no request.

use forgesync_core::document::DocumentRecipe;

use crate::config::{DocumentsConfig, EmbeddingServiceConfig, ForgesyncConfig};

#[test]
fn config_defaults_to_discussion_enriched_documents() {
    assert_eq!(
        ForgesyncConfig::default().documents,
        DocumentsConfig {
            recipe: DocumentRecipe::DiscussionEnriched
        }
    );
}

#[test]
fn config_accepts_both_explicit_document_recipes() {
    let original: ForgesyncConfig = toml::from_str("[documents]\nrecipe = 'original_body'\n")
        .expect("parse original-body config");
    let enriched: ForgesyncConfig = toml::from_str("[documents]\nrecipe = 'discussion_enriched'\n")
        .expect("parse discussion-enriched config");

    assert_eq!(original.documents.recipe, DocumentRecipe::OriginalBody);
    assert_eq!(
        enriched.documents.recipe,
        DocumentRecipe::DiscussionEnriched
    );
}

#[test]
fn config_rejects_unknown_recipe_values() {
    assert!(
        toml::from_str::<ForgesyncConfig>("[documents]\nrecipe = 'include_everything'\n").is_err()
    );
}

#[test]
fn embedding_service_defaults_are_bounded_and_independent() {
    let config = EmbeddingServiceConfig::default();
    config.validate().expect("default embedding settings");
    assert_eq!(config.endpoint, "https://api.openai.com/v1");
    assert_eq!(config.api_key_env, "OPENAI_API_KEY");
    assert_eq!(config.concurrency, 4);
}

#[test]
fn embedding_service_config_rejects_nonlocal_http() {
    let config = EmbeddingServiceConfig {
        endpoint: "http://example.com/v1".to_owned(),
        ..EmbeddingServiceConfig::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn embedding_service_config_rejects_batch_smaller_than_chunk() {
    let config = EmbeddingServiceConfig {
        max_batch_input_bytes: EmbeddingServiceConfig::default().max_input_bytes - 1,
        ..EmbeddingServiceConfig::default()
    };
    assert!(config.validate().is_err());
}
