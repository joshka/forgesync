//! Read and search scenario value builders; shared helpers live in `common`.

use std::num::NonZeroU32;

use forgesync_core::content::{Discussion, SourceState, ThreadKind};
use forgesync_core::identity::ThreadId;
use forgesync_core::provider_data::ProviderData;
use forgesync_store::reads::{ThreadQuery, ThreadSort, ThreadStateFilter};

pub use crate::common::{remove_archive, repository, temporary_archive_path, thread_id, timestamp};

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
