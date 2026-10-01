//! Shared filter parsing and conversion.

use clap::Parser;
use forgesync_core::content::ThreadKind;
use forgesync_engine::inspect::{ThreadSort, ThreadStateFilter};
use forgesync_engine::reference::RepositorySelector;

use crate::command::thread_filters::ThreadFilterArgs;

/// Minimal parser around the production filter arguments.
#[derive(Parser)]
struct FilterParser {
    #[command(flatten)]
    filters: ThreadFilterArgs,
}

#[test]
fn defaults_preserve_workflow_sort_selection() {
    let parsed = FilterParser::try_parse_from(["filters"]).expect("default arguments");
    let filters = parsed.filters.into_filters();

    assert!(filters.repositories.is_empty());
    assert_eq!(filters.kind, None);
    assert_eq!(filters.state, ThreadStateFilter::All);
    assert_eq!(filters.sort, None);
    assert_eq!(filters.limit, 20);
    assert_eq!(filters.offset, 0);
}

#[test]
fn explicit_scope_and_window_reach_engine_request() {
    let parsed = FilterParser::try_parse_from([
        "filters",
        "--repo",
        "owner/repo",
        "--kind",
        "pr",
        "--state",
        "closed",
        "--sort",
        "created",
        "--limit",
        "7",
        "--offset",
        "12",
    ])
    .expect("explicit arguments");
    let filters = parsed.filters.into_filters();
    let repository = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("expected selector");

    assert_eq!(filters.repositories, [repository]);
    assert_eq!(filters.kind, Some(ThreadKind::PullRequest));
    assert_eq!(filters.state, ThreadStateFilter::Closed);
    assert_eq!(filters.sort, Some(ThreadSort::Created));
    assert_eq!(filters.limit, 7);
    assert_eq!(filters.offset, 12);
}

#[rstest::rstest]
#[case::zero("0")]
#[case::over_bound("1001")]
fn parsing_rejects_out_of_range_limits(#[case] limit: &str) {
    let error = FilterParser::try_parse_from(["filters", "--limit", limit])
        .err()
        .expect("invalid limit");

    assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
}
