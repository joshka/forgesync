//! # Cluster scenario construction and database lifetime
//!
//! These fixtures build a repository, checked discussion identities, normalized issues, and
//! matching original-body documents. Title numbering comes from the checked identity, preventing
//! disagreement between the source payload and requested thread. The update timestamp is chosen by
//! each scenario.
//!
//! Construction performs no archive operations and computes no expected cluster results. Scenarios
//! reserve and apply observations, acquire and release fences, and persist vectors themselves.
//! Fixed source clocks make current-versus-stale representation selection reproducible.
//!
//! Filename allocation and closed-database cleanup are infrastructure helpers. The cleanup suffix
//! loop covers WAL sidecars only; it neither executes scenarios nor derives expected outcomes.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;

/// Distinguishes archive paths within this test process.
static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

/// Builds the original-body recipe matching the synthetic issue, without persisting it.
pub fn document(thread: &ThreadId, updated_at: UtcTimestamp) -> Document {
    let title = format!("Shared cache failure {}", thread.number().get());
    let text = format!("{title}\n\nThe cache fails after restart.");
    Document::new(
        thread.clone(),
        DocumentRecipe::OriginalBody,
        title,
        text.clone(),
        text.to_ascii_lowercase(),
        updated_at,
    )
}

/// Constructs a complete open-issue payload without reserving or applying evidence.
///
/// The checked identity supplies title numbering; the caller selects the current source update.
pub fn discussion(thread: &ThreadId, updated_at: UtcTimestamp) -> Discussion {
    let title = format!("Shared cache failure {}", thread.number().get());
    Discussion {
        id: thread.clone(),
        kind: ThreadKind::Issue,
        state: SourceState::Open,
        title,
        body: Some("The cache fails after restart.".to_owned()),
        html_url: None,
        created_at: timestamp("2026-09-19T10:00:00Z"),
        updated_at,
        closed_at: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        provider_data: ProviderData::new(),
    }
}

/// Constructs the synthetic clustering repository without registering it in an archive.
pub fn repository() -> Repository {
    Repository {
        id: RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new("repo-cluster-workflow").expect("repository provider ID"),
        ),
        owner: "example".to_owned(),
        name: "clustering".to_owned(),
        full_name: "example/clustering".to_owned(),
        default_branch: Some("main".to_owned()),
        updated_at: Some(timestamp("2026-09-20T10:00:00Z")),
        provider_data: ProviderData::new(),
    }
}

/// Constructs a repository-scoped issue identity with the scenario-selected provider ID and number.
pub fn thread_id(repository: &RepositoryId, provider_id: &str, number: u64) -> ThreadId {
    ThreadId::new(
        repository.clone(),
        ProviderId::new(provider_id).expect("thread provider ID"),
        ThreadNumber::new(number).expect("thread number"),
    )
}

/// Parses a fixture clock value and reports invalid setup before workflow execution.
pub fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
}

/// Allocates a process-local unique filename without creating or opening an archive.
pub fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-engine-clusters-{}-{sequence}.sqlite",
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
