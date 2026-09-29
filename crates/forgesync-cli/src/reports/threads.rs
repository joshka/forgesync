//! Threads command presentation.

use std::process::ExitCode;

use forgesync_core::content::{ReviewState, SourceState, ThreadKind as DiscussionKind};
use forgesync_core::coverage::CoverageState;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_engine::search::SearchResultPage;
use forgesync_store::reads::{ThreadDetail, ThreadPage, ThreadTimelineEvent};

use crate::output::{SearchPageOutput, ThreadDetailOutput, ThreadPageOutput};
use crate::{OutputMode, render_success};

/// Renders a local discussion page in human or JSON form.
pub fn render_thread_page(json: OutputMode, command: &str, page: &ThreadPage) -> ExitCode {
    let output = ThreadPageOutput::from(page);
    render_success(json, command, &output, thread_page_summary)
}

/// Renders ranked results with their effective retrieval mode.
pub fn render_search_page(json: OutputMode, page: &SearchResultPage) -> ExitCode {
    let output = SearchPageOutput::from(page);
    render_success(json, "search", &output, search_page_summary)
}

/// Renders one archived discussion and its selected evidence.
pub fn render_thread_detail(json: OutputMode, detail: &ThreadDetail) -> ExitCode {
    let output = ThreadDetailOutput::from(detail);
    render_success(json, "thread show", &output, thread_detail_summary)
}

/// Formats pagination and discussion summaries for terminal output.
pub fn thread_page_summary(page: &ThreadPageOutput<'_>) -> String {
    let mut lines = vec!["REPOSITORY\tNUMBER\tKIND\tSTATE\tTITLE".to_owned()];
    for item in &page.items {
        let thread = item.thread;
        lines.push(format!(
            "{}\t{}\t{}\t{}\t{}",
            item.repository.full_name,
            thread.id.number().get(),
            discussion_kind_name(thread.kind),
            source_state_name(&thread.state),
            thread.title
        ));
    }
    if page.items.is_empty() {
        lines.push("No discussions matched.".to_owned());
    }
    lines.push("Coverage:".to_owned());
    lines.extend(page.coverage.iter().map(|coverage| {
        format!(
            "  {}: {} complete, {} incomplete, {} missing of {}",
            family_name(coverage.family),
            coverage.complete,
            coverage.incomplete,
            coverage.missing,
            coverage.applicable_threads
        )
    }));
    if let Some(next_offset) = page.next_offset {
        lines.push(format!("Next offset: {next_offset}"));
    }
    lines.join("\n")
}

/// Formats ranking, fallback, and result provenance for terminal output.
pub fn search_page_summary(page: &SearchPageOutput<'_>) -> String {
    let mut lines = vec![format!(
        "Mode: {:?} (requested {:?}), ranking: {:?}, sort: {:?}",
        page.mode, page.requested_mode, page.ranking, page.sort
    )];
    if let Some(reason) = page.fallback_reason {
        lines.push(format!("Keyword fallback: {reason}"));
    }
    lines.push("RANK\tSCORE\tREPOSITORY\tNUMBER\tKIND\tSTATE\tTITLE".to_owned());
    for (index, item) in page.items.iter().enumerate() {
        let score = item
            .score
            .map(|score| format!("{score:.6}"))
            .unwrap_or_else(|| "-".to_owned());
        lines.push(format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            index + 1,
            score,
            item.repository.full_name,
            item.thread.id.number().get(),
            discussion_kind_name(item.thread.kind),
            source_state_name(&item.thread.state),
            item.thread.title
        ));
    }
    if page.items.is_empty() {
        lines.push("No discussions matched.".to_owned());
    }
    lines.push("Coverage:".to_owned());
    lines.extend(page.coverage.iter().map(|coverage| {
        format!(
            "  {}: {} complete, {} incomplete, {} missing of {}",
            family_name(coverage.family),
            coverage.complete,
            coverage.incomplete,
            coverage.missing,
            coverage.applicable_threads
        )
    }));
    if let Some(next_offset) = page.next_offset {
        lines.push(format!("Next offset: {next_offset}"));
    }
    lines.join("\n")
}

/// Formats a discussion timeline and current family coverage.
pub fn thread_detail_summary(detail: &ThreadDetailOutput<'_>) -> String {
    let thread = detail.summary.thread;
    let mut lines = vec![format!(
        "{}/{}#{} — {}\nKind: {}\nState: {}\nUpdated: {}",
        detail.summary.repository.owner,
        detail.summary.repository.name,
        thread.id.number().get(),
        thread.title,
        discussion_kind_name(thread.kind),
        source_state_name(&thread.state),
        format_timestamp(thread.updated_at)
    )];
    if let Some(url) = &thread.html_url {
        lines.push(format!("URL: {url}"));
    }
    if let Some(body) = &thread.body {
        lines.push(String::new());
        lines.push(body.clone());
    }
    lines.push(String::new());
    lines.push("Coverage:".to_owned());
    lines.extend(detail.summary.coverage.iter().map(|coverage| {
        format!(
            "  {}: {}{}",
            family_name(coverage.family()),
            coverage_state_name(coverage.state()),
            if coverage.is_stale() { " (stale)" } else { "" }
        )
    }));
    for item in detail.pull_request_metadata {
        let metadata = &item.payload;
        lines.push(String::new());
        lines.push(format!(
            "Pull request: {}:{} -> {}:{} (head {}, draft: {}, merged: {})",
            repository_identity(metadata.head.repository.as_ref()),
            metadata.head.name,
            repository_identity(metadata.base.repository.as_ref()),
            metadata.base.name,
            metadata.head.sha,
            metadata.draft,
            metadata.merged
        ));
    }

    if !detail.timeline.is_empty() {
        lines.push(String::new());
        lines.push("Current timeline:".to_owned());
        for entry in detail.timeline {
            let time = entry
                .occurred_at
                .map(format_timestamp)
                .unwrap_or_else(|| "time unavailable".to_owned());
            let summary = match &entry.event {
                ThreadTimelineEvent::ThreadCreated { thread, title } => {
                    format!(
                        "{}#{} opened: {title}",
                        thread.repository().provider_id(),
                        thread.number().get()
                    )
                }
                ThreadTimelineEvent::ThreadClosed { thread } => {
                    format!(
                        "{}#{} closed",
                        thread.repository().provider_id(),
                        thread.number().get()
                    )
                }
                ThreadTimelineEvent::Comment { comment } => format!(
                    "comment by {}: {}",
                    comment.author.as_deref().unwrap_or("unknown author"),
                    comment.body
                ),
                ThreadTimelineEvent::Review { review } => format!(
                    "review {} by {}: {}{}",
                    review.id.provider_id(),
                    review
                        .reviewer
                        .as_ref()
                        .and_then(|reviewer| reviewer.login.as_deref())
                        .unwrap_or("unknown reviewer"),
                    review_state_name(&review.state),
                    review
                        .body
                        .as_deref()
                        .map_or(String::new(), |body| format!(" — {body}"))
                ),
                ThreadTimelineEvent::ReviewThread {
                    path,
                    is_resolved,
                    is_outdated,
                    ..
                } => format!(
                    "review thread {}: {}{}",
                    path.as_deref().unwrap_or("unknown path"),
                    if *is_resolved {
                        "resolved"
                    } else {
                        "unresolved"
                    },
                    if *is_outdated { ", outdated" } else { "" }
                ),
                ThreadTimelineEvent::ReviewThreadComment {
                    path,
                    is_resolved,
                    is_outdated,
                    comment,
                    ..
                } => format!(
                    "review comment on {} ({}{}), by {}: {}",
                    path.as_deref().unwrap_or("unknown path"),
                    if *is_resolved {
                        "resolved"
                    } else {
                        "unresolved"
                    },
                    if *is_outdated { ", outdated" } else { "" },
                    comment.author.as_deref().unwrap_or("unknown author"),
                    comment.body
                ),
            };
            lines.push(format!("  {time}: {summary}"));
        }
    }
    lines.join("\n")
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
