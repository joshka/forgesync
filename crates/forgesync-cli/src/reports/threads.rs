//! Discussion and search presentation, plus shared display vocabulary.
//!
//! Unknown provider state strings are preserved.

use forgesync_core::content::{ReviewState, ThreadKind as DiscussionKind};
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_engine::search::SearchResultPage;
use forgesync_store::reads::{ThreadDetail, ThreadPage};

use crate::error::Exit;
use crate::output::{Output, SearchPageOutput, ThreadDetailOutput, ThreadPageOutput};
use crate::reports::detail::thread_detail_summary;

pub fn render_thread_page(output: Output, page: &ThreadPage) -> Exit {
    output.success(&ThreadPageOutput::from(page), ThreadPageOutput::summary)
}

pub fn render_search_page(output: Output, page: &SearchResultPage) -> Exit {
    output.success(&SearchPageOutput::from(page), SearchPageOutput::summary)
}

pub fn render_thread_detail(output: Output, detail: &ThreadDetail) -> Exit {
    output.success(&ThreadDetailOutput::from(detail), thread_detail_summary)
}

pub fn discussion_kind_name(kind: DiscussionKind) -> &'static str {
    match kind {
        DiscussionKind::Issue => "issue",
        DiscussionKind::PullRequest => "pull request",
    }
}

/// Returns a familiar label, or the unrecognized provider state unchanged.
pub fn review_state_name(state: &ReviewState) -> &str {
    match state {
        ReviewState::Approved => "approved",
        ReviewState::ChangesRequested => "changes requested",
        ReviewState::Commented => "commented",
        ReviewState::Dismissed => "dismissed",
        ReviewState::Pending => "pending",
        ReviewState::Other(value) => value,
    }
}

/// Formats host/provider coordinates for diagnostics; not an owner/name selector or URL.
pub fn repository_identity(repository: Option<&forgesync_core::identity::RepositoryId>) -> String {
    repository.map_or_else(
        || "unknown repository".to_owned(),
        |repository| {
            format!(
                "{}/{}",
                repository.host(),
                repository.provider_id().as_str()
            )
        },
    )
}

/// Formats RFC 3339, falling back to `invalid timestamp` so a report stays renderable.
pub fn format_timestamp(timestamp: UtcTimestamp) -> String {
    timestamp
        .format_rfc3339()
        .unwrap_or_else(|_| "invalid timestamp".to_owned())
}
