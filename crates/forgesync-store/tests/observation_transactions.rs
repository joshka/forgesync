//! # Observation transaction integration
//!
//! This suite groups parent application, ordering, child-family completion, and rollback cases.
//! The store decides which acquired evidence becomes canonical and keeps incomplete attempts
//! visible. Child modules isolate each invariant while sharing the on-disk archive setup.

use std::cmp::Ordering;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::{
    CommitSha, GitHubHost, ObservationSequence, ProviderId, RepositoryId, ThreadId, ThreadNumber,
};
use forgesync_core::observation::{
    CollectionCompleteness, IncompleteReason, Observation, SourceClock,
};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::families::ChildFamilyObservation;
use forgesync_store::observations::{ObservationDisposition, StagedItem};
use forgesync_store::ordering::{
    compare_observation_order, compare_revision_observation_order, observation_sequence_order_value,
};
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[path = "observation_transactions/children.rs"]
mod children;
#[path = "observation_transactions/ordering.rs"]
mod ordering;
#[path = "observation_transactions/parents.rs"]
mod parents;
#[path = "observation_transactions/rollback.rs"]
mod rollback;

/// Creates an on-disk archive and registers the synthetic repository.
///
/// The returned thread identity is not persisted until a scenario applies its observation.
async fn create_archive_with_repository(path: &PathBuf) -> (Archive, RepositoryId, ThreadId) {
    let archive = Archive::create(path).await.expect("create archive");
    let repository_id = RepositoryId::new(
        GitHubHost::parse("github.com").expect("host"),
        ProviderId::new("repository-42").expect("repository provider ID"),
    );
    let repository = Repository {
        id: repository_id.clone(),
        owner: "example".to_owned(),
        name: "project".to_owned(),
        full_name: "example/project".to_owned(),
        default_branch: Some("main".to_owned()),
        updated_at: Some(timestamp("2026-09-20T10:00:00Z")),
        provider_data: ProviderData::new(),
    };
    archive
        .upsert_repository(&repository)
        .await
        .expect("insert repository");
    let thread_id = thread_id(&repository_id);
    (archive, repository_id, thread_id)
}

/// Constructs issue 101 under the supplied repository without writing a parent row.
fn thread_id(repository_id: &RepositoryId) -> ThreadId {
    ThreadId::new(
        repository_id.clone(),
        ProviderId::new("thread-101").expect("thread provider ID"),
        ThreadNumber::new(101).expect("thread number"),
    )
}

/// Builds an open issue payload with caller-selected title and provider update time.
///
/// Repository identity is checked against the thread; other content stays constant so scenarios
/// can isolate source ordering and completeness changes.
fn discussion(
    repository_id: &RepositoryId,
    thread_id: &ThreadId,
    updated_at: &str,
    title: &str,
) -> Discussion {
    assert_eq!(thread_id.repository(), repository_id);
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
fn thread_observation(
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
fn incomplete(received_items: u64) -> CollectionCompleteness {
    CollectionCompleteness::Incomplete {
        reason: IncompleteReason::Pagination,
        received_items,
    }
}

/// Pairs a synthetic provider identity with the exact staging payload supplied by the scenario.
fn item(id: &str, payload: serde_json::Value) -> StagedItem<serde_json::Value> {
    StagedItem {
        id: ProviderId::new(id).expect("provider item ID"),
        payload,
    }
}

/// Reserves a durable acquisition sequence at the supplied timestamp before evidence is built.
async fn reserve(archive: &Archive, started_at: &str) -> ObservationSequence {
    archive
        .reserve_observation_sequence(timestamp(started_at))
        .await
        .expect("reserve observation sequence")
}

/// Parses a fixture timestamp, failing immediately if the scenario contains invalid setup.
fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
}

/// Reads the single canonical thread title through a separate read-only SQL connection.
///
/// This observes committed state directly, independently of archive projection helpers.
async fn read_current_thread_title(path: &PathBuf) -> String {
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
/// It does not create or migrate an archive; callers close it before exercising archive writes.
async fn writable_pool(path: &PathBuf) -> sqlx::SqlitePool {
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
fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, AtomicOrdering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-observations-{}-{sequence}.sqlite",
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
