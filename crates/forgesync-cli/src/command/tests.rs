//! Command parsing contract: global options and selectors that could silently change scope.

use std::path::Path;

use clap::Parser;

use crate::command::values::{ColorChoice, LogFormat, RefreshAnalysisArg, SyncIncludeArg};
use crate::command::{CliArgs, Command};

#[test]
fn global_options_parse_together() {
    let args = CliArgs::try_parse_from([
        "forgesync",
        "--archive",
        "archive.db",
        "--config",
        "config.toml",
        "--json",
        "--color",
        "never",
        "--log-format",
        "json",
        "-vv",
        "archive",
        "status",
    ])
    .expect("global options should parse");

    assert_eq!(args.archive.as_deref(), Some(Path::new("archive.db")));
    assert_eq!(args.config.as_deref(), Some(Path::new("config.toml")));
    assert!(args.json);
    assert_eq!(args.color, ColorChoice::Never);
    assert_eq!(args.log_format, LogFormat::Json);
    assert_eq!(args.verbose, 2);
}

#[rstest::rstest]
#[case::default(&["forgesync", "--archive", "archive.db", "sync", "owner/repo"], vec![])]
#[case::comments(
    &["forgesync", "--archive", "archive.db", "sync", "owner/repo", "--with", "comments"],
    vec![SyncIncludeArg::Comments]
)]
#[case::comments_and_reviews(
    &["forgesync", "--archive", "archive.db", "sync", "owner/repo", "--with", "comments,reviews"],
    vec![SyncIncludeArg::Comments, SyncIncludeArg::Reviews]
)]
#[case::review_threads(
    &["forgesync", "--archive", "archive.db", "sync", "owner/repo", "--with", "review-threads"],
    vec![SyncIncludeArg::ReviewThreads]
)]
fn sync_families_are_selected_with_with(
    #[case] arguments: &[&str],
    #[case] expected: Vec<SyncIncludeArg>,
) {
    let args = CliArgs::try_parse_from(arguments).expect("sync arguments should parse");
    let Command::Sync(sync) = args.command else {
        panic!("expected sync command");
    };
    assert_eq!(sync.scope.with, expected);
}

#[rstest::rstest]
#[case::sync_only(
    &["forgesync", "--archive", "archive.db", "refresh", "owner/repo"],
    false,
    vec![]
)]
#[case::explicit_analysis(
    &["forgesync", "--archive", "archive.db", "refresh", "owner/repo", "--no-sync", "--analyze", "embeddings,clusters"],
    true,
    vec![RefreshAnalysisArg::Embeddings, RefreshAnalysisArg::Clusters]
)]
fn refresh_analysis_stages_are_explicit_and_comma_separated(
    #[case] arguments: &[&str],
    #[case] expected_no_sync: bool,
    #[case] expected_analysis: Vec<RefreshAnalysisArg>,
) {
    let args = CliArgs::try_parse_from(arguments).expect("refresh arguments should parse");
    let Command::Refresh(refresh) = args.command else {
        panic!("expected refresh command");
    };
    assert_eq!(refresh.no_sync, expected_no_sync);
    assert_eq!(refresh.analyze, expected_analysis);
}

#[rstest::rstest]
#[case::no_stage(&["forgesync", "refresh", "owner/repo", "--no-sync"])]
#[case::state_without_sync(
    &["forgesync", "refresh", "owner/repo", "--no-sync", "--analyze", "clusters", "--state", "open"]
)]
#[case::families_without_sync(
    &["forgesync", "refresh", "owner/repo", "--no-sync", "--analyze", "clusters", "--with", "comments"]
)]
fn refresh_rejects_inconsistent_stage_selection(#[case] arguments: &[&str]) {
    let error = CliArgs::try_parse_from(arguments).expect_err("inconsistent refresh selection");
    assert_eq!(error.exit_code(), 2);
}

#[rstest::rstest]
#[case::run_show(&["forgesync", "run", "show", "0"])]
#[case::run_retry(&["forgesync", "run", "retry", "0"])]
#[case::cluster_show(&["forgesync", "cluster", "show", "0"])]
#[case::embed_chunk_below_validation(
    &["forgesync", "embed", "owner/repo", "--max-input-bytes", "3"]
)]
fn non_positive_or_out_of_range_values_are_usage_errors(#[case] arguments: &[&str]) {
    let error = CliArgs::try_parse_from(arguments).expect_err("invalid value");
    assert_eq!(error.exit_code(), 2);
}
