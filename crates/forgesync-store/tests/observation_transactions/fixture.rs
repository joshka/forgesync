//! Observation scenario value builders; shared path, pool, and lease helpers live in `common`.

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::identity::{ObservationSequence, ProviderId, RepositoryId, ThreadId};
use forgesync_core::observation::{CollectionCompleteness, IncompleteReason, ThreadObservation};
use forgesync_core::provider_data::ProviderData;
use forgesync_store::observations::StagedItem;

pub use crate::common::{remove_archive, temporary_archive_path, timestamp, writable_pool};

/// Constructs synthetic repository identity and metadata without creating or registering an
/// archive.
pub fn repository() -> Repository {
    crate::common::repository("example", "project", "repository-42")
}

/// Constructs issue 101 under the supplied repository without writing a parent row.
pub fn thread_id(repository_id: &RepositoryId) -> ThreadId {
    crate::common::thread_id(repository_id, "thread-101", 101)
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

/// Constructs complete thread evidence acquired at `observed_at` under `sequence`.
pub fn thread_observation(
    discussion: Discussion,
    observed_at: &str,
    sequence: ObservationSequence,
) -> ThreadObservation {
    ThreadObservation {
        discussion,
        observed_at: timestamp(observed_at),
        sequence,
    }
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
