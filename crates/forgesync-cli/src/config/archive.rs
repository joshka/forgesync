//! # Select the user's normal archive
//!
//! [`ArchiveConfig`] supplies one optional database path, shared by all repository workflows.
//! [`ArchiveConfig::resolve`] gives an invocation override precedence over that setting, then uses
//! the user's data directory. This selection never opens, creates, or migrates a database.
//!
//! Configuration loading anchors a relative TOML path to the containing config directory. Command
//! overrides instead retain their normal working-directory meaning. Neither path expands shell
//! variables or `~`; callers can supply an absolute path when selecting a database elsewhere.
//!
//! Default locations follow CLI conventions: XDG directories on Unix (including macOS), and the
//! Windows roaming application-data directory. The config file and database have separate homes;
//! deleting a settings file must not imply deleting the archive. Only explicit archive creation
//! creates parent directories. Store opening remains an existing-file operation.

use std::path::{Path, PathBuf};

use etcetera::{BaseStrategy, choose_base_strategy};
use serde::Deserialize;

use crate::config::ConfigError;

/// The single normal database used when an invocation does not override it.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ArchiveConfig {
    /// Optional database location; relative TOML paths are anchored during config loading.
    /// Omission selects `forgesync/archive.sqlite` within the user's data directory.
    pub path: Option<PathBuf>,
}

impl ArchiveConfig {
    /// Selects the invocation override, configured path, or default user-data database.
    ///
    /// Performs no filesystem mutation. Empty selected paths are configuration errors; an absent
    /// default location reports a directory-discovery error rather than using the current folder.
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

    /// Anchors a supplied relative database path to the selected TOML file's directory.
    ///
    /// Validates before joining, so an empty value cannot become the config directory itself.
    /// Absolute paths remain unchanged; this does not canonicalize or require an existing file.
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

/// Returns the automatically discovered user configuration file, without creating it.
pub fn default_config_path() -> Result<PathBuf, ConfigError> {
    let strategy = choose_base_strategy().map_err(ConfigError::UserDirectories)?;
    Ok(strategy.config_dir().join("forgesync/config.toml"))
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
