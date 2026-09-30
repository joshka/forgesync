//! # Local archive fixture reads and lifetime
//!
//! This module owns unique filenames, lease-clock setup, cleanup, and small local projections.
//! Scenarios still create and close archives explicitly and invoke real workflow operations.
//! None of the read helpers acquires provider data or changes durable state.
//!
//! Summary lookup assumes the fixture contains only owner/repo; discussion numbers therefore
//! identify a unique thread. Canonical child reads exclude staged incomplete collections.
//! Coverage selectors merely locate rows so each scenario must assert the state it expects.
//! The enumeration count is bounded to twenty rows, suitable for its one/two-thread fixtures.
//! Cleanup runs after close and tolerates sidecars already removed by SQLite.
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use forgesync_core::content::{Comment, Review, ReviewThread};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use forgesync_store::reads::ThreadQuery;
/// Distinguishes database paths within the test process.
static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

/// Reads a discussion by number from the first 1,000 archive threads, including coverage.
///
/// These fixtures contain owner/repo only, so repository-local numbers identify them uniquely.
/// A missing discussion or failed local read fails setup; this performs no acquisition or writes.
pub async fn thread_summary(
    archive: &Archive,
    number: u64,
) -> forgesync_store::reads::ThreadSummary {
    archive
        .query_threads(&ThreadQuery {
            repositories: Vec::new(),
            kind: None,
            state: forgesync_store::reads::ThreadStateFilter::All,
            match_expression: None,
            updated_since: None,
            sort: forgesync_store::reads::ThreadSort::Updated,
            limit: NonZeroU32::new(1000).expect("positive limit"),
            offset: 0,
        })
        .await
        .expect("query all threads")
        .items
        .into_iter()
        .find(|thread| thread.discussion.id.number().get() == number)
        .expect("thread summary")
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

/// Reads canonical issue-comment bodies in archive membership order for a discussion number.
///
/// Staged partial collections are excluded by the archive API; this projection does not establish
/// provider identity or acquisition completeness by itself.
pub async fn comment_bodies(archive: &Archive, number: u64) -> Vec<String> {
    let summary = thread_summary(archive, number).await;
    archive
        .child_family_members::<Comment>(&summary.discussion.id, EvidenceFamily::Comments)
        .await
        .expect("read canonical comments")
        .into_iter()
        .map(|item| item.payload.body)
        .collect()
}

/// Reads canonical review members, retaining their provider identities and payloads.
///
/// This reads local state only; failed or partial staged snapshots are not canonical membership.
pub async fn review_members(
    archive: &Archive,
    number: u64,
) -> Vec<forgesync_store::observations::StagedItem<Review>> {
    let summary = thread_summary(archive, number).await;
    archive
        .child_family_members::<Review>(&summary.discussion.id, EvidenceFamily::Reviews)
        .await
        .expect("read canonical reviews")
}

/// Reads canonical review-thread members, retaining identities and payloads.
///
/// This reads local state only; failed or partial staged snapshots are not canonical membership.
pub async fn review_thread_members(
    archive: &Archive,
    number: u64,
) -> Vec<forgesync_store::observations::StagedItem<ReviewThread>> {
    let summary = thread_summary(archive, number).await;
    archive
        .child_family_members::<ReviewThread>(&summary.discussion.id, EvidenceFamily::ReviewThreads)
        .await
        .expect("read canonical review threads")
}

/// Counts the first twenty local threads for the small enumeration fixtures.
///
/// This is a bounded page count, not an archive-wide count. The scenarios expect one or two threads
/// and would also fail if the page filled to its bound.
pub async fn thread_count(archive: &Archive) -> usize {
    archive
        .query_threads(&ThreadQuery {
            repositories: Vec::new(),
            kind: None,
            state: forgesync_store::reads::ThreadStateFilter::All,
            match_expression: None,
            updated_since: None,
            sort: forgesync_store::reads::ThreadSort::Updated,
            limit: NonZeroU32::new(20).expect("positive limit"),
            offset: 0,
        })
        .await
        .expect("query threads")
        .items
        .len()
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
