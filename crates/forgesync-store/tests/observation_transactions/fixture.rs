//! # Observation integration setup and value construction
//!
//! Repository and thread constructors supply fixed checked identities without archive writes.
//! Scenarios create the database and register that repository explicitly.
//! Value helpers construct discussion payloads, independent clocks, completeness, and staging rows.
//! Scenarios reserve every durable acquisition sequence before applying their evidence.
//!
//! Raw read-only inspection checks committed titles independently of public projection helpers.
//! Writable pools arrange trigger failures or corruption without creating or migrating databases.
//! Helpers contain no scenario assertions or expected ordering calculations.
//! Filename allocation separates concurrent cases; cleanup visits a fixed SQLite sidecar list.
//! Scenario files own observation application, staging, completion, and the state they expect.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::{
    GitHubHost, ObservationSequence, ProviderId, RepositoryId, ThreadId, ThreadNumber,
};
use forgesync_core::observation::{
    CollectionCompleteness, IncompleteReason, Observation, SourceClock,
};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::observations::StagedItem;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

/// Separates database filenames for concurrently executing cases in this process.
static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

/// Constructs synthetic repository identity and metadata without creating or registering an
/// archive.
pub fn repository() -> Repository {
    let repository_id = RepositoryId::new(
        GitHubHost::parse("github.com").expect("host"),
        ProviderId::new("repository-42").expect("repository provider ID"),
    );
    Repository {
        id: repository_id.clone(),
        owner: "example".to_owned(),
        name: "project".to_owned(),
        full_name: "example/project".to_owned(),
        default_branch: Some("main".to_owned()),
        updated_at: Some(timestamp("2026-09-20T10:00:00Z")),
        provider_data: ProviderData::new(),
    }
}

/// Constructs issue 101 under the supplied repository without writing a parent row.
pub fn thread_id(repository_id: &RepositoryId) -> ThreadId {
    ThreadId::new(
        repository_id.clone(),
        ProviderId::new("thread-101").expect("thread provider ID"),
        ThreadNumber::new(101).expect("thread number"),
    )
}

/// Builds an open issue payload with caller-selected title and provider update time.
///
/// Repository identity comes from the thread itself; other content stays constant so scenarios
/// can isolate source ordering and completeness changes.
pub fn discussion(thread_id: &ThreadId, updated_at: &str, title: &str) -> Discussion {
    Discussion {
        id: thread_id.clone(),
        kind: ThreadKind::Issue,
        state: SourceState::Open,
        title: title.to_owned(),
        body: Some("body".to_owned()),
        html_url: Some("https://github.com/example/project/issues/101".to_owned()),
        created_at: timestamp("2026-09-19T09:00:00Z"),
        updated_at: timestamp(updated_at),
        closed_at: None,
        labels: vec!["triage".to_owned()],
        assignees: Vec::new(),
        provider_data: ProviderData::new(),
    }
}

/// Constructs thread evidence while keeping source time, acquisition time, sequence, and
/// completeness independently controlled by the scenario. No sequence is reserved here.
pub fn thread_observation(
    discussion: Discussion,
    source_clock: &str,
    observed_at: &str,
    sequence: ObservationSequence,
    completeness: CollectionCompleteness,
) -> Observation<Discussion> {
    Observation::new(
        EvidenceFamily::Threads,
        discussion,
        SourceClock::from_raw(Some(source_clock)),
        timestamp(observed_at),
        sequence,
        completeness,
    )
}

/// Marks a received collection as unfinished because pagination did not complete.
pub fn incomplete(received_items: u64) -> CollectionCompleteness {
    CollectionCompleteness::Incomplete {
        reason: IncompleteReason::Pagination,
        received_items,
    }
}

/// Pairs a synthetic provider identity with the exact staging payload supplied by the scenario.
pub fn item(id: &str, payload: serde_json::Value) -> StagedItem<serde_json::Value> {
    StagedItem {
        id: ProviderId::new(id).expect("provider item ID"),
        payload,
    }
}

/// Parses a fixture timestamp, failing immediately if the scenario contains invalid setup.
pub fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
}

/// Reads the single canonical thread title through a separate read-only SQL connection.
///
/// This observes committed state directly, independently of archive projection helpers.
pub async fn read_current_thread_title(path: &PathBuf) -> String {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false)
                .read_only(true)
                .foreign_keys(true),
        )
        .await
        .expect("open inspection pool");
    let title = sqlx::query_scalar("SELECT title FROM threads")
        .fetch_one(&pool)
        .await
        .expect("read canonical title");
    pool.close().await;
    title
}

/// Opens an existing database for scenario-specific trigger installation or corruption setup.
///
/// It does not create or migrate an archive. Scenarios may retain the pool to remove an injected
/// trigger after an archive write, but never hold a SQL transaction across that write. Close the
/// pool before deleting the database and its sidecars.
pub async fn writable_pool(path: &PathBuf) -> sqlx::SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false)
                .foreign_keys(true),
        )
        .await
        .expect("open trigger pool")
}

/// Allocates a process-local unique filename without creating or opening an archive.
pub fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, AtomicOrdering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-observations-{}-{sequence}.sqlite",
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
