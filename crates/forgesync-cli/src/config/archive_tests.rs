//! Archive path precedence and config-relative interpretation.

use std::path::{Path, PathBuf};

use crate::config::ConfigError;
use crate::config::archive::ArchiveConfig;

#[test]
fn invocation_override_wins_over_configured_archive() {
    let config = ArchiveConfig {
        path: Some(PathBuf::from("normal.sqlite")),
    };
    let selected = config
        .resolve(Some(Path::new("temporary.sqlite")))
        .expect("select override");
    assert_eq!(selected, Path::new("temporary.sqlite"));
}

#[test]
fn configured_archive_needs_no_invocation_override() {
    let config = ArchiveConfig {
        path: Some(PathBuf::from("normal.sqlite")),
    };
    let selected = config.resolve(None).expect("select configured path");
    assert_eq!(selected, Path::new("normal.sqlite"));
}

#[test]
fn relative_archive_is_anchored_to_the_config_directory() {
    let mut config = ArchiveConfig {
        path: Some(PathBuf::from("data/archive.sqlite")),
    };
    config
        .relative_to_config(Path::new("settings/config.toml"))
        .expect("anchor relative path");
    assert_eq!(
        config.path,
        Some(PathBuf::from("settings/data/archive.sqlite"))
    );
}

#[test]
fn empty_configured_archive_is_rejected_before_anchoring() {
    let mut config = ArchiveConfig {
        path: Some(PathBuf::new()),
    };
    let error = config
        .relative_to_config(Path::new("settings/config.toml"))
        .expect_err("reject empty path");
    assert!(matches!(error, ConfigError::EmptyArchivePath));
}

#[test]
fn empty_override_is_rejected_instead_of_selecting_the_configured_archive() {
    let config = ArchiveConfig {
        path: Some(PathBuf::from("normal.sqlite")),
    };
    let error = config
        .resolve(Some(Path::new("")))
        .expect_err("reject empty override");
    assert!(matches!(error, ConfigError::EmptyArchivePath));
}
