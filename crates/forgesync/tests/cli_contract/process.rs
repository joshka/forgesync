//! # Process and exit contract
//!
//! These cases invoke package version, global help, missing-subcommand usage, and the optional TUI
//! terminal requirement. Arguments and stream/status expectations remain visible in each case.
//! Help exposes only the selected application scope and reflects whether TUI support was built.
//!
//! The TUI case deliberately supplies invalid configuration while using noninteractive process
//! streams. Terminal validation must precede configuration loading, so that configuration cannot
//! obscure the actionable terminal error. It is compiled only with the TUI feature.
//! These cases do not establish partial engine-report rendering; workflow suites cover those
//! reports. Version output is checked against this package's build metadata rather than a prefix.

#[cfg(feature = "tui")]
use forgesync_store::archive::Archive;

use crate::forgesync;
#[cfg(feature = "tui")]
use crate::{remove_archive, temporary_archive_path};

#[test]
fn version_flag_prints_package_version() {
    let output = forgesync().arg("--version").output().expect("run binary");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("version is UTF-8");
    assert_eq!(stdout, format!("forgesync {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_lists_global_options_and_no_deferred_commands() {
    let output = forgesync().arg("--help").output().expect("run binary");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help is UTF-8");
    assert!(stdout.contains("--archive"));
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--json"));
    assert!(stdout.contains("--color"));
    assert!(stdout.contains("--log-format"));
    assert!(!stdout.contains("portable"));
    assert!(!stdout.contains("cloud"));
    assert!(!stdout.contains("summarize"));
    assert!(!stdout.contains("code index"));
    assert!(!stdout.contains("serve"));
    assert!(stdout.contains("search"));
    assert!(stdout.contains("thread"));
    assert!(stdout.contains("run"));
    assert!(stdout.contains("embed"));
    assert!(stdout.contains("cluster"));
    assert_eq!(stdout.contains("tui"), cfg!(feature = "tui"));
}

#[test]
fn json_does_not_change_clap_usage_errors() {
    let output = forgesync().arg("--json").output().expect("run binary");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic is UTF-8");
    assert!(stderr.contains("requires a subcommand"));
}

#[cfg(feature = "tui")]
#[tokio::test]
async fn tui_requires_an_interactive_terminal() {
    let path = temporary_archive_path();
    let config_path = path.with_extension("toml");
    std::fs::write(&config_path, "[documents]\nrecipe = 'unknown'\n")
        .expect("write irrelevant invalid config");
    let archive = Archive::create(&path).await.expect("create archive");
    archive.close().await;

    let output = forgesync()
        .args(["tui", "--archive"])
        .arg(&path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("run tui without a terminal");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("diagnostic is UTF-8");
    assert!(stderr.contains("requires an interactive terminal"));
    assert!(!stderr.contains("config file"));
    remove_archive(&path);
    let _ = std::fs::remove_file(config_path);
}
