//! # Read and search construction fixtures
//!
//! Value helpers supply normalized discussion fields, repository identity, and query settings.
//! Keyword query construction fixes incidental pagination and sort choices without reading rows.
//! No helper reserves sequences, applies observations, executes search, or opens an archive.
//! Scenario-selected kind, state, text, and time remain visible where content is constructed.
//!
//! Filename allocation and closed-database cleanup are the only filesystem operations here.
//! The fixed sidecar loop releases resources rather than enumerating test scenarios.
//! Scenarios own registration, writes, migration, inspection, and expected results.
//! Engine ranking policy and CLI presentation belong to their respective suites.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::reads::{ThreadQuery, ThreadSort, ThreadStateFilter};

/// Separates database filenames for concurrent scenarios in this process.
static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

/// Constructs the first ten all-repository keyword matches ranked by relevance.
///
/// It performs no query; scenarios invoke the archive and assert results directly.
pub fn keyword_query(expression: &str) -> ThreadQuery {
    ThreadQuery {
        repositories: Vec::new(),
        kind: None,
        state: ThreadStateFilter::All,
        match_expression: Some(expression.to_owned()),
        updated_since: None,
        sort: ThreadSort::Relevance,
        limit: NonZeroU32::new(10).expect("positive limit"),
        offset: 0,
    }
}

/// Constructs the scenario-selected identity, kind, state, searchable text, and update time.
///
/// Other metadata stays constant. This is value construction only, with no archive writes.
pub fn discussion(
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
pub fn repository(owner: &str, name: &str, provider_id: &str) -> Repository {
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
pub fn thread_id(repository: &RepositoryId, provider_id: &str, number: u64) -> ThreadId {
    ThreadId::new(
        repository.clone(),
        ProviderId::new(provider_id).expect("thread provider ID"),
        ThreadNumber::new(number).expect("thread number"),
    )
}

/// Parses a fixture timestamp, failing on invalid setup before the archive operation.
pub fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
}

/// Allocates a process-local unique filename without creating or opening an archive.
pub fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-store-{}-{sequence}.sqlite",
        std::process::id()
    ))
}

/// Removes the closed database and its possible WAL sidecars on a best-effort basis.
///
/// The fixed suffix loop is cleanup only; it does not select scenarios or compute expectations.
pub fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}
