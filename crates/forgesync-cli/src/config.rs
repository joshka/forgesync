use std::path::{Path, PathBuf};

use forgesync_core::DocumentRecipe;
use serde::Deserialize;
use thiserror::Error;

/// User configuration for Forgesync operations.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ForgesyncConfig {
    /// Inputs used to create retrieval documents and embeddings.
    pub documents: DocumentsConfig,
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
}

impl ConfigError {
    /// Stable machine-readable CLI error classification.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Read { .. } => "config_read_failed",
            Self::Parse { .. } => "config_invalid",
        }
    }
}

fn config_path_from_environment() -> Option<PathBuf> {
    std::env::var_os("FORGESYNC_CONFIG").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::{DocumentsConfig, ForgesyncConfig};
    use forgesync_core::DocumentRecipe;

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
}
