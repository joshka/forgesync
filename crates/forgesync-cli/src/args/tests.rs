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

#[test]
fn sync_families_are_selected_with_with() {
    let args =
        CliArgs::try_parse_from(["forgesync", "--archive", "archive.db", "sync", "owner/repo"])
            .expect("sync without comments should parse");
    assert!(matches!(
        args.command,
        Command::Sync {
            with,
            ..
        } if with.is_empty()
    ));

    let args = CliArgs::try_parse_from([
        "forgesync",
        "--archive",
        "archive.db",
        "sync",
        "owner/repo",
        "--with",
        "comments",
    ])
    .expect("sync with comments should parse");
    assert!(matches!(
        args.command,
        Command::Sync {
            with,
            ..
        } if with == vec![SyncIncludeArg::Comments]
    ));

    let args = CliArgs::try_parse_from([
        "forgesync",
        "--archive",
        "archive.db",
        "sync",
        "owner/repo",
        "--with",
        "comments,reviews",
    ])
    .expect("sync with reviews should parse");
    assert!(matches!(
        args.command,
        Command::Sync { with, .. }
            if with == vec![SyncIncludeArg::Comments, SyncIncludeArg::Reviews]
    ));

    let args = CliArgs::try_parse_from([
        "forgesync",
        "--archive",
        "archive.db",
        "sync",
        "owner/repo",
        "--with",
        "review-threads",
    ])
    .expect("sync with review threads should parse");
    assert!(matches!(
        args.command,
        Command::Sync { with, .. }
            if with == vec![SyncIncludeArg::ReviewThreads]
    ));
}

#[test]
fn refresh_analysis_stages_are_explicit_and_comma_separated() {
    let args = CliArgs::try_parse_from([
        "forgesync",
        "--archive",
        "archive.db",
        "refresh",
        "owner/repo",
    ])
    .expect("sync-only refresh should parse");
    assert!(matches!(args.command, Command::Refresh { analyze, .. } if analyze.is_empty()));

    let args = CliArgs::try_parse_from([
        "forgesync",
        "--archive",
        "archive.db",
        "refresh",
        "owner/repo",
        "--no-sync",
        "--analyze",
        "embeddings,clusters",
    ])
    .expect("explicit analysis stages should parse");
    assert!(matches!(
        args.command,
        Command::Refresh { no_sync: true, analyze, .. }
            if analyze == vec![RefreshAnalysisArg::Embeddings, RefreshAnalysisArg::Clusters]
    ));
}
