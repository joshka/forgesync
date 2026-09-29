//! # Prepare configured embedding clients at the process boundary
//!
//! `EmbeddingServiceConfig::client` resolves the selected environment key and validates client
//! settings before constructing the adapter. `EmbeddingSetupError` retains whether configuration
//! or client initialization failed, so each command can preserve its exit-status policy.
//!
//! Embed requires a usable client. Refresh can treat setup failure as an unavailable optional
//! stage. Search requires a client only for semantic or hybrid retrieval. This common preparation
//! performs no provider request; engine workflows own actual service use and cancellation.
//!
//! Configuration precedence is resolved before calling this method, including embed overrides.

use forgesync_engine::embedding_client::{EmbeddingClient, EmbeddingClientError};

use crate::config::{ConfigError, EmbeddingServiceConfig};

impl EmbeddingServiceConfig {
    /// Resolves the configured secret and prepares a validated client without making a request.
    pub fn client(&self) -> Result<EmbeddingClient, EmbeddingSetupError> {
        self.validate()
            .map_err(EmbeddingSetupError::Configuration)?;
        let api_key = std::env::var(&self.api_key_env).unwrap_or_default();
        let config = self
            .client_config(api_key)
            .map_err(EmbeddingSetupError::Configuration)?;
        EmbeddingClient::new(config).map_err(EmbeddingSetupError::Client)
    }
}

/// Setup phase retained for command-specific usage versus execution error handling.
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingSetupError {
    /// File, environment, or command settings do not describe a usable service.
    #[error("{0}")]
    Configuration(#[source] ConfigError),
    /// Validated settings could not initialize the transport adapter.
    #[error("{0}")]
    Client(#[source] EmbeddingClientError),
}
impl EmbeddingSetupError {
    /// Preserves the underlying stable code in the CLI error envelope.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Configuration(error) => error.code(),
            Self::Client(error) => error.code(),
        }
    }
}
