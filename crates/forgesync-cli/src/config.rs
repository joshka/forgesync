//! Load local application configuration.
//!
//! [`ForgesyncConfig::load`] reads an explicit path (`--config` or `FORGESYNC_CONFIG`), or the
//! user configuration file, falling back to defaults only when that automatic file is absent.
//! Omitted fields receive defaults and unknown fields are rejected.
//!
//! Embedding settings are validated only by workflows that need them, so local archive inspection
//! does not require a usable service configuration. Store and engine libraries receive explicit
//! values and never read the process environment.

use std::path::{Path, PathBuf};
use std::time::Duration;

use forgesync_core::document::DocumentRecipe;
use forgesync_engine::embedding_client::{EmbeddingClient, EmbeddingClientConfig};
use forgesync_engine::refresh::EmbeddingServiceIdentity;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use crate::credentials::valid_environment_variable_name;
use crate::error::CliError;

pub mod archive;

use archive::{ArchiveConfig, default_config_path};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ForgesyncConfig {
    pub archive: ArchiveConfig,
    pub documents: DocumentsConfig,
    pub embeddings: EmbeddingServiceConfig,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct DocumentsConfig {
    pub recipe: DocumentRecipe,
}

impl Default for DocumentsConfig {
    fn default() -> Self {
        Self {
            recipe: DocumentRecipe::DiscussionEnriched,
        }
    }
}

/// OpenAI-compatible embedding service settings.
///
/// Byte limits govern UTF-8 input size, not token counts or service capacity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct EmbeddingServiceConfig {
    /// Base URL before the standard `/embeddings` path is appended.
    pub endpoint: String,
    pub model: String,
    /// Environment variable containing the API key; the key itself is never stored in TOML.
    pub api_key_env: String,
    pub dimensions: Option<u32>,
    /// Maximum UTF-8 bytes in one input chunk.
    pub max_input_bytes: usize,
    /// Maximum UTF-8 bytes across one request batch.
    pub max_batch_input_bytes: usize,
    /// Maximum chunks sent in one request.
    pub batch_size: usize,
    /// Maximum requests in flight.
    pub concurrency: usize,
    pub request_timeout_seconds: u64,
    /// Total retry budget per request.
    pub retry_budget_seconds: u64,
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
    /// Checks every setting and returns the parsed endpoint.
    ///
    /// Requires HTTPS except for HTTP to `localhost` or a loopback IP, and rejects URL
    /// credentials, query, and fragment. No request is sent and no environment is read.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidEmbeddings`] naming the first invalid setting.
    pub fn validate(&self) -> Result<Url, ConfigError> {
        let invalid = |field| ConfigError::InvalidEmbeddings { field };
        let endpoint = Url::parse(&self.endpoint)
            .ok()
            .filter(valid_endpoint)
            .ok_or(invalid("endpoint"))?;
        let checks = [
            ("model", !self.model.trim().is_empty()),
            (
                "api_key_env",
                valid_environment_variable_name(&self.api_key_env),
            ),
            (
                "dimensions",
                self.dimensions
                    .is_none_or(|dimensions| (1..=65_536).contains(&dimensions)),
            ),
            ("max_input_bytes", self.max_input_bytes >= 4),
            (
                "max_batch_input_bytes",
                (self.max_input_bytes..=300_000).contains(&self.max_batch_input_bytes),
            ),
            ("batch_size", (1..=2048).contains(&self.batch_size)),
            ("concurrency", (1..=64).contains(&self.concurrency)),
            (
                "request_timeout_seconds",
                (1..=600).contains(&self.request_timeout_seconds),
            ),
            (
                "retry_budget_seconds",
                (1..=3600).contains(&self.retry_budget_seconds),
            ),
            ("max_attempts", (1..=8).contains(&self.max_attempts)),
        ];
        match checks.into_iter().find(|(_, valid)| !valid) {
            Some((field, _)) => Err(invalid(field)),
            None => Ok(endpoint),
        }
    }

    /// Endpoint and model identity that selects compatible stored vectors; reads no key.
    pub fn identity(&self) -> Result<EmbeddingServiceIdentity, ConfigError> {
        let endpoint = self.validate()?;
        Ok(EmbeddingServiceIdentity {
            endpoint: endpoint.as_str().trim_end_matches('/').to_owned(),
            model: self.model.trim().to_owned(),
        })
    }

    /// Converts validated settings into engine client settings with a resolved API key.
    pub fn client_config(&self, api_key: String) -> Result<EmbeddingClientConfig, ConfigError> {
        Ok(EmbeddingClientConfig {
            endpoint: self.validate()?,
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

    /// Reads the API key from the named environment variable and prepares a client.
    ///
    /// A missing key is reported by the client when it first sends a request.
    pub fn client(&self) -> Result<EmbeddingClient, CliError> {
        let api_key = std::env::var(&self.api_key_env).unwrap_or_default();
        Ok(EmbeddingClient::new(self.client_config(api_key)?)?)
    }
}

impl ForgesyncConfig {
    /// Loads `explicit_path`, or the user config file when none is given.
    ///
    /// Relative archive paths are anchored to the config file's directory.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Read`] for a selected file that cannot be read, including a missing
    /// explicit file, or [`ConfigError::Parse`] for malformed TOML or unknown settings.
    pub fn load(explicit_path: Option<&Path>) -> Result<Self, ConfigError> {
        if let Some(path) = explicit_path {
            return Self::load_file(path);
        }
        let path = default_config_path()?;
        match Self::load_file(&path) {
            Err(ConfigError::Read { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(Self::default())
            }
            result => result,
        }
    }

    fn load_file(path: &Path) -> Result<Self, ConfigError> {
        let source = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let mut config: Self = toml::from_str(&source).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        config.archive.relative_to_config(path)?;
        Ok(config)
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not locate the user's configuration/data directories")]
    UserDirectories(#[source] etcetera::HomeDirError),
    #[error("archive path must not be empty")]
    EmptyArchivePath,
    #[error("could not read config file {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("config file {path} is invalid")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("embedding service setting `{field}` is invalid")]
    InvalidEmbeddings { field: &'static str },
}

impl ConfigError {
    /// Stable machine-readable CLI error classification.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UserDirectories(_) => "config_directory_unavailable",
            Self::Read { .. } => "config_read_failed",
            Self::EmptyArchivePath | Self::Parse { .. } | Self::InvalidEmbeddings { .. } => {
                "config_invalid"
            }
        }
    }
}

fn valid_endpoint(endpoint: &Url) -> bool {
    let local_http =
        endpoint.scheme() == "http" && endpoint.host_str().is_some_and(is_loopback_host);
    (endpoint.scheme() == "https" || local_http)
        && endpoint.host_str().is_some()
        && endpoint.username().is_empty()
        && endpoint.password().is_none()
        && endpoint.query().is_none()
        && endpoint.fragment().is_none()
}

/// Accepts `localhost` or a loopback IP, including bracketed IPv6.
fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests;
