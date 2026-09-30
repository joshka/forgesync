//! # Offline presentation scenarios
//!
//! Human detail and status expose source, coverage, timeline, and diagnostics in reader order.
//! Each scenario builds an independent on-disk archive with one known discussion.
//! Process arguments and expected output are visible beside the operation under test.
//! No helper executes a query or chooses a scenario on behalf of these cases.
//!
//! Detail checks exact source, coverage, and timeline text for the fixed fixture.
//! Status checks stable sections while allowing generated lease and schema diagnostics.
//! Separate query cases check JSON results and reported archive-state stability.
//! Construction and cleanup live in `fixture`; store and engine suites own query mechanics.

use crate::fixture::{forgesync, remove_archive, seed_archive, temporary_archive_path};

/// Human detail keeps source content ahead of completeness limits and current timeline evidence.
#[tokio::test]
async fn thread_detail_human_sections_keep_source_coverage_and_timeline_order() {
    let path = temporary_archive_path();
    seed_archive(&path).await;
    let output = forgesync()
        .args(["thread", "show", "example/project#17", "--archive"])
        .arg(&path)
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .expect("show human detail offline");
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("UTF-8 human detail");
    assert_eq!(
        text,
        concat!(
            "example/project#17 — Issues OR cache timeout\n",
            "Kind: issue\nState: open\nUpdated: 2026-09-20T10:00:00Z\n",
            "URL: https://github.com/example/project/issues/17\n\n",
            "A cache issue appears after a network timeout.\n\n",
            "Coverage:\n  threads: complete\n  comments: missing\n\n",
            "Current timeline:\n  2026-09-19T10:00:00Z: repository-17#17 opened: Issues OR cache timeout\n",
        )
    );
    remove_archive(&path);
}

/// Human status keeps its section order and distinguishes local work from source coverage.
#[tokio::test]
async fn archive_status_human_sections_keep_counts_coverage_and_diagnostics_order() {
    let path = temporary_archive_path();
    seed_archive(&path).await;
    let output = forgesync()
        .arg("--archive")
        .arg(&path)
        .args(["archive", "status"])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .expect("run human archive status offline");
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("UTF-8 human archive status");
    let (_, sections) = text
        .split_once("Repositories:")
        .expect("archive counts section");
    let (counts, coverage) = sections
        .split_once("Coverage:\n")
        .expect("coverage section");
    assert_eq!(counts, " 1\nThreads: 1 (1 issues, 0 pull requests)\n");
    let (coverage, diagnostics) = coverage.split_once("Work:").expect("work section");
    assert!(coverage.contains("threads: 1 complete, 0 incomplete, 0 missing of 1"));
    assert!(diagnostics.starts_with(
        " 0 unresolved failures, 0 failed jobs, 0 deferred jobs, 0 in-progress runs\nLease: available (fence 0, expires "
    ));
    assert!(diagnostics.contains("\nSchema: "));
    assert!(diagnostics.ends_with(" supported (history valid)\n"));
    remove_archive(&path);
}
