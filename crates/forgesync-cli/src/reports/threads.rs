//! Discussion and search presentation, plus shared display vocabulary.
//!
//! Unknown provider state strings are preserved. Coverage labels name the state variant only;
//! staleness and failure details must be presented separately when needed.

use forgesync_core::content::{ReviewState, SourceState, ThreadKind as DiscussionKind};
use forgesync_core::coverage::CoverageState;
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

/// Returns the display name for a normalized discussion kind.
pub fn discussion_kind_name(kind: DiscussionKind) -> &'static str {
    match kind {
        DiscussionKind::Issue => "issue",
        DiscussionKind::PullRequest => "pull request",
    }
}

/// Returns a familiar label or the retained unrecognized provider state.
///
/// The borrowed fallback preserves source vocabulary; it is not normalized or escaped here.
pub fn source_state_name(state: &SourceState) -> &str {
    match state {
        SourceState::Open => "open",
        SourceState::Closed => "closed",
        SourceState::Other(value) => value,
    }
}

/// Returns a familiar review label or the retained unrecognized provider state.
///
/// Unknown values remain visible instead of being presented as a known review decision.
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

/// Formats stable host/provider coordinates, or an explicit missing-identity label.
///
/// This diagnostic representation is not an owner/name selector or a repository URL.
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

/// Returns the display name of an evidence family.
pub fn family_name(family: forgesync_core::coverage::EvidenceFamily) -> &'static str {
    match family {
        forgesync_core::coverage::EvidenceFamily::Threads => "threads",
        forgesync_core::coverage::EvidenceFamily::Comments => "comments",
        forgesync_core::coverage::EvidenceFamily::PullRequestMetadata => "pull_request_metadata",
        forgesync_core::coverage::EvidenceFamily::Reviews => "reviews",
        forgesync_core::coverage::EvidenceFamily::ReviewThreads => "review_threads",
    }
}

/// Returns the coverage variant label without its associated evidence or failure details.
///
/// Staleness is a separate coverage property and must be displayed by the caller when relevant.
pub fn coverage_state_name(state: &CoverageState) -> &'static str {
    match state {
        CoverageState::Missing => "missing",
        CoverageState::Incomplete { .. } => "incomplete",
        CoverageState::Complete { .. } => "complete",
        CoverageState::Unavailable { .. } => "unavailable",
        CoverageState::Failed { .. } => "failed",
        CoverageState::Deferred { .. } => "deferred",
    }
}

/// Formats a UTC instant as RFC 3339, falling back to `invalid timestamp` on format failure.
///
/// The fallback keeps a human report renderable; callers needing typed formatting failures should
/// use the timestamp API directly.
pub fn format_timestamp(timestamp: UtcTimestamp) -> String {
    timestamp
        .format_rfc3339()
        .unwrap_or_else(|_| "invalid timestamp".to_owned())
}
