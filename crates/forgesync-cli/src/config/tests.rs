//! Configuration parsing and service-boundary examples.
//!
//! These cases distinguish default selection, explicit document recipes, parse rejection, and
//! semantic embedding validation. They construct settings directly rather than mutating process
//! environment, keeping the expected input visible and safe under concurrent tests. Network and
//! credential resolution belong to their own boundaries; successful validation makes no request.
//!
//! Recipe cases select explicit TOML and expected domain values. Invalid endpoint and capacity
//! cases assert the configuration category rather than accepting any failure. Default checks
//! establish selected public values and validation, not exhaustive coverage of every service limit.

use forgesync_core::document::DocumentRecipe;

use crate::config::{ConfigError, DocumentsConfig, EmbeddingServiceConfig, ForgesyncConfig};

#[test]
fn config_defaults_to_discussion_enriched_documents() {
    assert_eq!(
        ForgesyncConfig::default().documents,
        DocumentsConfig {
            recipe: DocumentRecipe::DiscussionEnriched
        }
    );
}

#[rstest::rstest]
#[case::original_body(
    "[documents]\nrecipe = 'original_body'\n",
    DocumentRecipe::OriginalBody
)]
#[case::discussion_enriched(
    "[documents]\nrecipe = 'discussion_enriched'\n",
    DocumentRecipe::DiscussionEnriched
)]
fn config_accepts_explicit_document_recipe(#[case] input: &str, #[case] expected: DocumentRecipe) {
    let config: ForgesyncConfig = toml::from_str(input).expect("parse selected recipe");
    assert_eq!(config.documents.recipe, expected);
}

#[test]
fn config_rejects_unknown_recipe_values() {
    let result = toml::from_str::<ForgesyncConfig>("[documents]\nrecipe = 'include_everything'\n");
    let error = result.expect_err("reject unknown recipe");
    assert!(error.message().contains("unknown variant"));
    assert!(error.message().contains("include_everything"));
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
    let error = config.validate().expect_err("reject remote HTTP endpoint");
    assert!(matches!(error, ConfigError::InvalidEmbeddings));
}

#[test]
fn embedding_service_config_rejects_batch_smaller_than_chunk() {
    let config = EmbeddingServiceConfig {
        max_batch_input_bytes: EmbeddingServiceConfig::default().max_input_bytes - 1,
        ..EmbeddingServiceConfig::default()
    };
    let error = config
        .validate()
        .expect_err("reject incompatible input limits");
    assert!(matches!(error, ConfigError::InvalidEmbeddings));
}
