//! Parsed embedding command arguments.

use clap::{ArgAction, Args};
use forgesync_engine::reference::RepositorySelector;

/// Build documents and store compatible embeddings for local discussions.
#[derive(Clone, Debug, Args)]
pub struct EmbedArgs {
    /// One or more registered repositories to embed.
    #[arg(value_name = "OWNER/REPO", required = true)]
    pub repositories: Vec<RepositorySelector>,
    /// Force provider requests even when current compatible vectors are stored.
    #[arg(long, action = ArgAction::SetTrue)]
    pub force: bool,
    /// Override the configured OpenAI-compatible base endpoint.
    #[arg(long, value_name = "URL")]
    pub endpoint: Option<String>,
    /// Override the configured embedding model.
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,
    /// Override the environment variable name containing the API key.
    #[arg(long, value_name = "NAME")]
    pub api_key_env: Option<String>,
    /// Override the expected output dimensions.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=65536))]
    pub dimensions: Option<u32>,
    /// Override the maximum UTF-8 bytes per input chunk.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=300000))]
    pub max_input_bytes: Option<u32>,
    /// Override the maximum UTF-8 bytes in one request.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=300000))]
    pub max_batch_input_bytes: Option<u32>,
    /// Override the maximum inputs per request.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=2048))]
    pub batch_size: Option<u32>,
    /// Override the maximum requests in flight for this service.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=64))]
    pub concurrency: Option<u32>,
}
