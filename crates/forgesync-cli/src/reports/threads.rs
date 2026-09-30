//! # Render local discussion views
//!
//! Thread report functions select human or JSON presentation for list pages, search hits, and
//! detail. They consume engine and store projections so the formatting path stays free of provider
//! calls and SQL.
//!
//! Detail output includes source content and evidence coverage. Search output includes result
//! provenance, helping users understand whether a hit came from text, vectors, or both.

use std::process::ExitCode;

use forgesync_core::content::{ReviewState, SourceState, ThreadKind as DiscussionKind};
use forgesync_core::coverage::CoverageState;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_engine::search::SearchResultPage;
use forgesync_store::reads::{ThreadDetail, ThreadPage};

use crate::output::{SearchPageOutput, ThreadDetailOutput, ThreadPageOutput};
use crate::reports::detail::thread_detail_summary;
use crate::{OutputMode, render_success};

/// Renders a local discussion page in human or JSON form.
pub fn render_thread_page(json: OutputMode, command: &str, page: &ThreadPage) -> ExitCode {
    let output = ThreadPageOutput::from(page);
    render_success(json, command, &output, ThreadPageOutput::summary)
}

/// Renders ranked results with their effective retrieval mode.
pub fn render_search_page(json: OutputMode, page: &SearchResultPage) -> ExitCode {
    let output = SearchPageOutput::from(page);
    render_success(json, "search", &output, SearchPageOutput::summary)
}

/// Renders one archived discussion and its selected evidence.
pub fn render_thread_detail(json: OutputMode, detail: &ThreadDetail) -> ExitCode {
    let output = ThreadDetailOutput::from(detail);
    render_success(json, "thread show", &output, thread_detail_summary)
}

/// Returns the display name for a normalized discussion kind.
pub fn discussion_kind_name(kind: DiscussionKind) -> &'static str {
    match kind {
        DiscussionKind::Issue => "issue",
        DiscussionKind::PullRequest => "pull request",
    }
}

/// Returns the display name of a provider source state.
pub fn source_state_name(state: &SourceState) -> &str {
    match state {
        SourceState::Open => "open",
        SourceState::Closed => "closed",
        SourceState::Other(value) => value,
    }
}

/// Returns the display name of a normalized review state.
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

/// Formats an optional repository identity for diagnostic output.
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

/// Returns the display label of a family coverage state.
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

/// Formats a validated UTC instant for human output.
pub fn format_timestamp(timestamp: UtcTimestamp) -> String {
    timestamp
        .format_rfc3339()
        .unwrap_or_else(|_| "invalid timestamp".to_owned())
}
