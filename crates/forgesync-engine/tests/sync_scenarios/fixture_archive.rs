//! # Archive reference construction, coverage selection, and lifetime
//!
//! This module constructs references for the host-qualified fixture repository, provider ID 41.
//! Scenarios read current thread detail directly rather than searching a bounded page of all
//! threads and assuming that a discussion number is globally unique. It performs no archive reads
//! or writes.
//!
//! Coverage selectors locate one already-read family record. They do not assert completeness or
//! freshness; each scenario compares its own expected state beside the engine operation.
//! Enumeration counts come from explicit archive-status reads in those scenarios.
//!
//! Unique paths, live lease-clock values, and best-effort closed-database cleanup are
//! infrastructure helpers. Source content uses fixed clocks; leases need a time that agrees with
//! competing writers. Cleanup tolerates WAL sidecars already removed by SQLite.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::{
    GitHubHost, ProviderId, RepositoryId, ThreadNumber, ThreadReference,
};
use forgesync_core::timestamp::UtcTimestamp;
/// Distinguishes database paths within the test process.
static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

/// Constructs a reference in the fixture's host-qualified repository, provider ID 41.
///
/// The repository response in `fixture_issues` defines this identity. Construction performs no
/// read; each scenario invokes `Archive::thread_detail` beside its own assertions without bounded
/// search.
pub fn thread_reference(number: u64) -> ThreadReference {
    let host = GitHubHost::parse("github.com").expect("fixture host");
    let provider_id = ProviderId::new("41").expect("fixture repository provider ID");
    let repository = RepositoryId::new(host, provider_id);
    let number = ThreadNumber::new(number).expect("fixture thread number");
    ThreadReference::new(repository, number)
}

/// Selects the existing comment-family evidence row without asserting freshness or completeness.
///
/// The scenario must assert its state; a missing row fails the test immediately.
pub fn comment_coverage(
    summary: &forgesync_store::reads::ThreadSummary,
) -> &forgesync_core::coverage::Coverage {
    summary
        .coverage
        .iter()
        .find(|coverage| coverage.family() == EvidenceFamily::Comments)
        .expect("comment coverage")
}

/// Selects the existing review-family evidence row without asserting freshness or completeness.
///
/// The scenario must assert its state; a missing row fails the test immediately.
pub fn review_coverage(
    summary: &forgesync_store::reads::ThreadSummary,
) -> &forgesync_core::coverage::Coverage {
    summary
        .coverage
        .iter()
        .find(|coverage| coverage.family() == EvidenceFamily::Reviews)
        .expect("review coverage")
}

/// Selects the existing review-thread evidence row without asserting freshness or completeness.
///
/// The scenario must assert its state; a missing row fails the test immediately.
pub fn review_thread_coverage(
    summary: &forgesync_store::reads::ThreadSummary,
) -> &forgesync_core::coverage::Coverage {
    summary
        .coverage
        .iter()
        .find(|coverage| coverage.family() == EvidenceFamily::ReviewThreads)
        .expect("review-thread coverage")
}

/// Reserves a process-local unique SQLite filename without creating an archive.
///
/// Each scenario explicitly creates and closes its archive before cleanup.
pub fn temporary_archive_path() -> PathBuf {
    let next = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-sync-workflow-{}-{next}.sqlite",
        std::process::id()
    ))
}

/// Reads wall-clock UTC at microsecond precision for archive lease fixtures.
///
/// Lease time must agree with the competing process clock; other content fixtures use fixed source
/// times. Clock-before-epoch and out-of-range values fail fixture setup.
pub fn current_timestamp() -> UtcTimestamp {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after Unix epoch");
    UtcTimestamp::from_unix_microseconds(
        i64::try_from(elapsed.as_micros()).expect("current timestamp fits"),
    )
    .expect("valid current timestamp")
}

/// Removes a closed SQLite archive and its WAL/shared-memory sidecars.
///
/// Missing paths are expected when SQLite has already cleaned up. Removal is best effort and does
/// not conceal the assertions that establish durable behavior.
pub fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}
