//! # Present archived discussions and search results
//!
//! [`render_thread_page`], [`render_search_page`], and [`render_thread_detail`] adapt existing
//! engine or store projections into the public CLI output DTOs. The shared renderer selects text
//! or JSON and translates rendering failures into a process result. These adapters perform no
//! archive reads, provider calls, ranking, or filtering of their own.
//!
//! Page and search summaries belong to their output DTOs; the larger detail layout lives in the
//! sibling detail report module. Search output retains the effective retrieval mode and provenance
//! so users can interpret keyword, vector, or fused results. Detail retains source content and
//! evidence coverage rather than treating every displayed child collection as complete.
//!
//! The remaining helpers provide shared display vocabulary for discussion kinds, source and review
//! states, family coverage, repository identities, and timestamps. Unknown provider state strings
//! are preserved. Coverage labels describe the state variant only: they do not include staleness,
//! failure details, or acquisition provenance, which callers must present separately when needed.
//!
//! Repository diagnostics use stable host/provider identity rather than an owner/name selector;
//! absent identity has an explicit fallback. Timestamp formatting likewise has a defensive display
//! fallback. These human labels are presentation choices, not parsers or durable identity formats.

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
