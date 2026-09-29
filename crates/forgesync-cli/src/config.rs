//! # Load local application configuration
//!
//! `ForgesyncConfig` groups archive, document, and embedding-service settings. Nested config types
//! supply defaults and validation for the parts of the app that need them; `ConfigError` reports
//! malformed or unusable configuration.
//!
//! The CLI resolves config before constructing requests. Store and engine libraries receive
//! explicit values and never reach into the process environment, which makes their behavior
//! repeatable for a given input.

use std::path::{Path, PathBuf};
use std::time::Duration;

use forgesync_core::document::DocumentRecipe;
use forgesync_engine::embedding_client::EmbeddingClientConfig;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use crate::credentials::valid_environment_variable_name;

/// User configuration for Forgesync operations.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ForgesyncConfig {
    /// Inputs used to create retrieval documents and embeddings.
    pub documents: DocumentsConfig,
    /// Independent OpenAI-compatible embedding service settings.
    pub embeddings: EmbeddingServiceConfig,
}

/// Document recipe selected by embedding and analysis operations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct DocumentsConfig {
    /// Recipe that supplies the current discussion text.
    pub recipe: DocumentRecipe,
}

impl Default for DocumentsConfig {
    fn default() -> Self {
        Self {
            recipe: DocumentRecipe::DiscussionEnriched,
        }
    }
}

/// Endpoint, model, key reference, and bounded request settings for embeddings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct EmbeddingServiceConfig {
    /// Base URL before the standard `/embeddings` path is appended.
    pub endpoint: String,
    /// Model name sent with embedding requests.
    pub model: String,
    /// Environment variable containing the API key; the key itself is never stored in TOML.
    pub api_key_env: String,
    /// Expected output dimensions, when configured for the selected model.
    pub dimensions: Option<u32>,
    /// Maximum UTF-8 bytes in one deterministic input chunk.
    pub max_input_bytes: usize,
    /// Maximum UTF-8 bytes across one request batch.
    pub max_batch_input_bytes: usize,
    /// Maximum chunks sent in one request.
    pub batch_size: usize,
    /// Maximum requests in flight for this service.
    pub concurrency: usize,
    /// Timeout per request, in seconds.
    pub request_timeout_seconds: u64,
    /// Total retry budget per request, in seconds.
    pub retry_budget_seconds: u64,
    /// Maximum attempts for transient failures.
    pub max_attempts: u32,
}

impl Default for EmbeddingServiceConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://api.openai.com/v1".to_owned(),
            model: "text-embedding-3-small".to_owned(),
            api_key_env: "OPENAI_API_KEY".to_owned(),
            dimensions: None,
            max_input_bytes: 7_000,
            max_batch_input_bytes: 250_000,
            batch_size: 64,
            concurrency: 4,
            request_timeout_seconds: 30,
            retry_budget_seconds: 120,
            max_attempts: 3,
        }
    }
}

impl EmbeddingServiceConfig {
    /// Validates endpoint and resource bounds before an embedding operation starts.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let endpoint = Url::parse(&self.endpoint).map_err(|_| ConfigError::InvalidEmbeddings)?;
        let secure = endpoint.scheme() == "https";
        let local_http =
            endpoint.scheme() == "http" && endpoint.host_str().is_some_and(is_loopback_host);
        let valid_endpoint = (secure || local_http)
            && endpoint.host_str().is_some()
            && endpoint.username().is_empty()
            && endpoint.password().is_none()
            && endpoint.query().is_none()
            && endpoint.fragment().is_none();
        if !valid_endpoint
            || self.model.trim().is_empty()
            || !valid_environment_variable_name(&self.api_key_env)
            || self.dimensions == Some(0)
            || self
                .dimensions
                .is_some_and(|dimensions| dimensions > 65_536)
            || self.max_input_bytes < 4
            || self.max_batch_input_bytes < self.max_input_bytes
            || self.max_batch_input_bytes > 300_000
            || self.batch_size == 0
            || self.batch_size > 2048
            || self.concurrency == 0
            || self.concurrency > 64
            || self.request_timeout_seconds == 0
            || self.request_timeout_seconds > 600
            || self.retry_budget_seconds == 0
            || self.retry_budget_seconds > 3600
            || self.max_attempts == 0
            || self.max_attempts > 8
        {
            return Err(ConfigError::InvalidEmbeddings);
        }
        Ok(())
    }

    /// Creates the HTTP client settings after the application resolves the secret key.
    pub fn client_config(&self, api_key: String) -> Result<EmbeddingClientConfig, ConfigError> {
        self.validate()?;
        let endpoint = Url::parse(&self.endpoint).map_err(|_| ConfigError::InvalidEmbeddings)?;
        Ok(EmbeddingClientConfig {
            endpoint,
            model: self.model.trim().to_owned(),
            api_key,
            dimensions: self.dimensions,
            max_input_bytes: self.max_input_bytes,
            max_batch_input_bytes: self.max_batch_input_bytes,
            batch_size: self.batch_size,
            concurrency: self.concurrency,
            request_timeout: Duration::from_secs(self.request_timeout_seconds),
            total_budget: Duration::from_secs(self.retry_budget_seconds),
            max_attempts: self.max_attempts,
        })
    }
}

impl ForgesyncConfig {
    /// Loads explicit or `FORGESYNC_CONFIG` TOML, or returns defaults when neither is set.
    pub fn load(explicit_path: Option<&Path>) -> Result<Self, ConfigError> {
        let path = explicit_path
            .map(Path::to_path_buf)
            .or_else(config_path_from_environment);
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let source = std::fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        toml::from_str(&source).map_err(|source| ConfigError::Parse { path, source })
    }
}

/// Error reading or parsing the user configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The selected config file could not be read.
    #[error("could not read config file {path}")]
    Read {
        /// Config path used by the caller.
        path: PathBuf,
        /// Filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// The selected config file contains invalid TOML or unknown settings.
    #[error("config file {path} is invalid")]
    Parse {
        /// Config path used by the caller.
        path: PathBuf,
        /// TOML error.
        #[source]
        source: toml::de::Error,
    },
    /// The embedding service has an invalid endpoint or request budget.
    #[error("embedding service configuration is invalid")]
    InvalidEmbeddings,
}

impl ConfigError {
    /// Stable machine-readable CLI error classification.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Read { .. } => "config_read_failed",
            Self::Parse { .. } | Self::InvalidEmbeddings => "config_invalid",
        }
    }
}

/// Reads the optional process override for the configuration file path.
fn config_path_from_environment() -> Option<PathBuf> {
    std::env::var_os("FORGESYNC_CONFIG").map(PathBuf::from)
}

/// Accepts a local model endpoint by host identity, including bracketed IPv6, for the relaxed
/// local HTTP configuration path.
fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests {
    use forgesync_core::document::DocumentRecipe;

    use super::{DocumentsConfig, EmbeddingServiceConfig, ForgesyncConfig};

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
        let enriched: ForgesyncConfig =
            toml::from_str("[documents]\nrecipe = 'discussion_enriched'\n")
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
            toml::from_str::<ForgesyncConfig>("[documents]\nrecipe = 'include_everything'\n")
                .is_err()
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
    fn embedding_service_config_rejects_unsafe_urls_and_invalid_budgets() {
        let config = EmbeddingServiceConfig {
            endpoint: "http://example.com/v1".to_owned(),
            ..EmbeddingServiceConfig::default()
        };
        assert!(config.validate().is_err());
        let config = EmbeddingServiceConfig {
            max_batch_input_bytes: EmbeddingServiceConfig::default().max_input_bytes - 1,
            ..EmbeddingServiceConfig::default()
        };
        assert!(config.validate().is_err());
    }
}
