//! Select the user's archive.
//!
//! A relative path in TOML is anchored to the config file's directory, while `--archive` keeps its
//! working-directory meaning. The config file and database have separate homes, so deleting
//! settings never implies deleting the archive. Selection never opens, creates, or migrates a
//! database.

use std::path::{Path, PathBuf};

use etcetera::{BaseStrategy, choose_base_strategy};
use serde::Deserialize;

use crate::config::ConfigError;

/// The single archive database used unless an invocation overrides it.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ArchiveConfig {
    /// Defaults to `forgesync/archive.sqlite` in the user's data directory.
    pub path: Option<PathBuf>,
}

impl ArchiveConfig {
    /// Selects the invocation override, configured path, or default user-data database.
    pub fn resolve(&self, override_path: Option<&Path>) -> Result<PathBuf, ConfigError> {
        let selected = override_path.or(self.path.as_deref());
        if let Some(path) = selected {
            if path.as_os_str().is_empty() {
                return Err(ConfigError::EmptyArchivePath);
            }
            return Ok(path.to_path_buf());
        }
        let strategy = choose_base_strategy().map_err(ConfigError::UserDirectories)?;
        Ok(strategy.data_dir().join("forgesync/archive.sqlite"))
    }

    /// Anchors a relative database path to the config file's directory.
    ///
    /// Validates before joining, so an empty value cannot become the config directory itself.
    pub fn relative_to_config(&mut self, config_path: &Path) -> Result<(), ConfigError> {
        let Some(path) = &mut self.path else {
            return Ok(());
        };
        if path.as_os_str().is_empty() {
            return Err(ConfigError::EmptyArchivePath);
        }
        if path.is_relative() {
            let directory = config_path.parent().unwrap_or_else(|| Path::new("."));
            *path = directory.join(&*path);
        }
        Ok(())
    }
}

/// Returns the user configuration file path without creating it.
pub fn default_config_path() -> Result<PathBuf, ConfigError> {
    let strategy = choose_base_strategy().map_err(ConfigError::UserDirectories)?;
    Ok(strategy.config_dir().join("forgesync/config.toml"))
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
