use clap::Parser;

use super::{CliArgs, ColorChoice, Command, LogFormat, RefreshAnalysisArg, SyncIncludeArg};

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

    assert_eq!(
        args.archive.as_deref().and_then(|path| path.to_str()),
        Some("archive.db")
    );
    assert_eq!(
        args.config.as_deref().and_then(|path| path.to_str()),
        Some("config.toml")
    );
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
    assert_eq!(sync.with, expected);
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
