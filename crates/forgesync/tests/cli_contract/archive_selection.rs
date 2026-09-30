//! # One configured archive across normal process invocations
//!
//! Independent subprocesses select their config/data directories through child environment values.
//! Each scenario keeps configuration creation, command execution, and assertions visible. The
//! parent's environment and actual user database are never changed. Explicit configs use relative
//! archive names to establish anchoring without platform-specific path escaping.
//!
//! These cases prove default initialization, automatic config discovery, an invocation override,
//! explicit-config precedence, and missing/invalid-file failures. They also show that reads do not
//! create default folders. Store suites establish schema and transaction behavior beyond selection.

use crate::{forgesync, temporary_archive_path};

#[test]
fn default_initialization_creates_one_database_without_path_flags() {
    let root = temporary_archive_path().with_extension("directory");
    let database = root.join("forgesync/archive.sqlite");
    let output = forgesync()
        .env("XDG_CONFIG_HOME", &root)
        .env("XDG_DATA_HOME", &root)
        .env("APPDATA", &root)
        .args(["archive", "init", "--json"])
        .output()
        .expect("initialize default archive");
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).expect("init JSON");
    assert!(output.status.success());
    assert_eq!(
        envelope["data"]["path"],
        database.to_string_lossy().as_ref()
    );
    assert!(database.exists());
    let status = forgesync()
        .env("XDG_CONFIG_HOME", &root)
        .env("XDG_DATA_HOME", &root)
        .env("APPDATA", &root)
        .args(["archive", "status", "--json"])
        .output()
        .expect("read same default archive");
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).expect("status JSON");
    assert_eq!(
        status["data"]["archive"]["archive_id"],
        envelope["data"]["archive_id"]
    );
    std::fs::remove_dir_all(root).expect("remove closed fixture");
}

#[test]
fn automatic_config_selects_a_relative_database_without_path_flags() {
    let root = temporary_archive_path().with_extension("directory");
    let directory = root.join("forgesync");
    std::fs::create_dir_all(&directory).expect("create config directory");
    std::fs::write(
        directory.join("config.toml"),
        "[archive]\npath = 'selected.sqlite'\n",
    )
    .expect("write config");
    let output = forgesync()
        .env("XDG_CONFIG_HOME", &root)
        .env("APPDATA", &root)
        .args(["archive", "init", "--json"])
        .output()
        .expect("initialize configured archive");
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).expect("init JSON");
    assert!(output.status.success());
    assert_eq!(
        envelope["data"]["path"],
        directory.join("selected.sqlite").to_string_lossy().as_ref()
    );
    assert!(!directory.join("archive.sqlite").exists());
    std::fs::remove_dir_all(root).expect("remove closed fixture");
}

#[test]
fn invocation_archive_overrides_the_configured_database() {
    let root = temporary_archive_path().with_extension("directory");
    std::fs::create_dir_all(&root).expect("create config directory");
    let config = root.join("settings.toml");
    let override_path = root.join("override.sqlite");
    std::fs::write(&config, "[archive]\npath = 'normal.sqlite'\n").expect("write config");
    let output = forgesync()
        .args(["archive", "init", "--json", "--config"])
        .arg(config)
        .arg("--archive")
        .arg(&override_path)
        .output()
        .expect("initialize override");
    assert!(output.status.success());
    assert!(override_path.exists());
    assert!(!root.join("normal.sqlite").exists());
    std::fs::remove_dir_all(root).expect("remove closed fixture");
}

#[test]
fn environment_config_selects_the_normal_database() {
    let root = temporary_archive_path().with_extension("directory");
    std::fs::create_dir_all(&root).expect("create config directory");
    let config = root.join("settings.toml");
    std::fs::write(&config, "[archive]\npath = 'normal.sqlite'\n").expect("write config");
    let output = forgesync()
        .env("FORGESYNC_CONFIG", config)
        .args(["archive", "init", "--json"])
        .output()
        .expect("initialize environment-configured archive");
    assert!(output.status.success());
    assert!(root.join("normal.sqlite").exists());
    std::fs::remove_dir_all(root).expect("remove closed fixture");
}

#[test]
fn explicit_config_wins_over_an_environment_config() {
    let root = temporary_archive_path().with_extension("directory");
    std::fs::create_dir_all(&root).expect("create config directory");
    let config = root.join("settings.toml");
    std::fs::write(&config, "[archive]\npath = 'normal.sqlite'\n").expect("write config");
    let output = forgesync()
        .env("FORGESYNC_CONFIG", root.join("missing.toml"))
        .args(["archive", "init", "--json", "--config"])
        .arg(config)
        .output()
        .expect("initialize explicit config");
    assert!(output.status.success());
    assert!(root.join("normal.sqlite").exists());
    std::fs::remove_dir_all(root).expect("remove closed fixture");
}

#[test]
fn reading_an_absent_default_database_creates_no_directories() {
    let root = temporary_archive_path().with_extension("directory");
    let output = forgesync()
        .env("XDG_CONFIG_HOME", &root)
        .env("XDG_DATA_HOME", &root)
        .env("APPDATA", &root)
        .args(["archive", "status", "--json"])
        .output()
        .expect("read absent archive");
    assert_eq!(output.status.code(), Some(1));
    assert!(!root.exists());
}

#[test]
fn malformed_automatic_config_does_not_fall_back_to_another_database() {
    let root = temporary_archive_path().with_extension("directory");
    let directory = root.join("forgesync");
    std::fs::create_dir_all(&directory).expect("create config directory");
    std::fs::write(directory.join("config.toml"), "[archive]\npath = ''\n")
        .expect("write invalid path");
    let output = forgesync()
        .env("XDG_CONFIG_HOME", &root)
        .env("XDG_DATA_HOME", &root)
        .env("APPDATA", &root)
        .args(["archive", "init", "--json"])
        .output()
        .expect("reject invalid config");
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).expect("error JSON");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(envelope["error"]["code"], "config_invalid");
    assert!(!directory.join("archive.sqlite").exists());
    std::fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn explicitly_selected_missing_config_is_an_error() {
    let root = temporary_archive_path().with_extension("directory");
    let output = forgesync()
        .args(["archive", "init", "--json", "--config"])
        .arg(root.join("missing.toml"))
        .output()
        .expect("reject missing explicit config");
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).expect("error JSON");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(envelope["error"]["code"], "config_read_failed");
    assert!(!root.exists());
}
