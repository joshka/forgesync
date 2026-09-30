//! # Store read and search integration
//!
//! This suite groups local projections over an archive: detail, filtered lists, full-text search,
//! and migration-sensitive reads. Its child modules isolate SQL-facing behavior so a failure
//! points to the affected read path. Engine and CLI suites cover request policy and presentation.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use forgesync_store::reads::{ThreadQuery, ThreadSort, ThreadStateFilter};

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[path = "inspect_search/detail.rs"]
mod detail;
#[path = "inspect_search/fts.rs"]
mod fts;
#[path = "inspect_search/list_search.rs"]
mod list_search;
#[path = "inspect_search/migration.rs"]
mod migration;

/// Reads the first ten all-repository keyword matches ranked by relevance.
///
/// This helper fixes incidental page settings; the expression and result assertions remain visible
/// in the scenario. It performs no writes or provider requests.
async fn keyword_page(archive: &Archive, expression: &str) -> forgesync_store::reads::ThreadPage {
    archive
        .query_threads(&ThreadQuery {
            repositories: Vec::new(),
            kind: None,
            state: ThreadStateFilter::All,
            match_expression: Some(expression.to_owned()),
            updated_since: None,
            sort: ThreadSort::Relevance,
            limit: NonZeroU32::new(10).expect("positive limit"),
            offset: 0,
        })
        .await
        .expect("query thread page")
}

/// Reserves a sequence and commits complete thread evidence for the supplied discussion.
///
/// Provider update time also supplies acquisition time in this fixture. Store application updates
/// the derived full-text representation; callers register the repository before invoking this.
async fn apply_thread(archive: &Archive, discussion: Discussion) {
    let updated_at = discussion.updated_at;
    let sequence = archive
        .reserve_observation_sequence(updated_at)
        .await
        .expect("reserve sequence");
    let raw_source_clock = updated_at.format_rfc3339().expect("format timestamp");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        discussion,
        SourceClock::from_raw(Some(&raw_source_clock)),
        updated_at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply thread observation");
}

/// Constructs the scenario-selected identity, kind, state, searchable text, and update time.
///
/// Other metadata stays constant. This is value construction only, with no archive writes.
fn discussion(
    thread: &ThreadId,
    kind: ThreadKind,
    state: SourceState,
    title: &str,
    body: Option<&str>,
    updated_at: &str,
) -> Discussion {
    Discussion {
        id: thread.clone(),
        kind,
        state,
        title: title.to_owned(),
        body: body.map(str::to_owned),
        html_url: None,
        created_at: timestamp("2026-09-19T10:00:00Z"),
        updated_at: timestamp(updated_at),
        closed_at: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        provider_data: ProviderData::new(),
    }
}

/// Constructs public GitHub repository metadata without registering it in an archive.
fn repository(owner: &str, name: &str, provider_id: &str) -> Repository {
    Repository {
        id: RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new(provider_id).expect("repository provider ID"),
        ),
        owner: owner.to_owned(),
        name: name.to_owned(),
        full_name: format!("{owner}/{name}"),
        default_branch: Some("main".to_owned()),
        updated_at: Some(timestamp("2026-09-20T10:00:00Z")),
        provider_data: ProviderData::new(),
    }
}

/// Constructs a discussion identity scoped to its repository with explicit provider ID and number.
fn thread_id(repository: &RepositoryId, provider_id: &str, number: u64) -> ThreadId {
    ThreadId::new(
        repository.clone(),
        ProviderId::new(provider_id).expect("thread provider ID"),
        ThreadNumber::new(number).expect("thread number"),
    )
}

/// Parses a fixture timestamp, failing on invalid setup before the archive operation.
fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
}

/// Allocates a process-local unique filename without creating or opening an archive.
fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-store-{}-{sequence}.sqlite",
        std::process::id()
    ))
}

/// Removes the closed database and its possible WAL sidecars on a best-effort basis.
///
/// The fixed suffix loop is cleanup only; it does not select scenarios or compute expectations.
fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}
