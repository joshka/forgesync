//! # Present an archived discussion in meaningful sections
//!
//! `ThreadDetailOutput` supplies borrowed source and evidence projections. Its section methods
//! build source, coverage, pull-request metadata, and timeline lines independently. Their order
//! puts evidence limits before content that readers might otherwise assume is complete.
//!
//! `threads` owns entry-point rendering and JSON selection. This module owns human detail text;
//! it neither loads the archive nor changes JSON DTOs. Timeline text describes current evidence,
//! not a complete source revision history. Formatting helpers remain shared with list reports.
//!
//! CLI output regressions cover these labels and the distinction between missing and empty data.

use forgesync_store::reads::ThreadTimelineEvent;

use super::threads::{
    coverage_state_name, discussion_kind_name, family_name, format_timestamp, repository_identity,
    review_state_name, source_state_name,
};
use crate::output::ThreadDetailOutput;

/// Formats source content, coverage, pull-request context, and timeline in that order.
pub fn thread_detail_summary(detail: &ThreadDetailOutput<'_>) -> String {
    let mut lines = detail.source_lines();
    lines.extend(detail.coverage_lines());
    lines.extend(detail.metadata_lines());
    lines.extend(detail.timeline_lines());
    lines.join("\n")
}

impl ThreadDetailOutput<'_> {
    /// Shows source identity and body independently of acquired child evidence.
    fn source_lines(&self) -> Vec<String> {
        let thread = self.summary.thread;
        let mut lines = vec![format!(
            "{}/{}#{} — {}\nKind: {}\nState: {}\nUpdated: {}",
            self.summary.repository.owner,
            self.summary.repository.name,
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
        lines
    }
    /// Explains missing or stale family evidence before presenting it.
    fn coverage_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(String::new());
        lines.push("Coverage:".to_owned());
        lines.extend(self.summary.coverage.iter().map(|coverage| {
            format!(
                "  {}: {}{}",
                family_name(coverage.family()),
                coverage_state_name(coverage.state()),
                if coverage.is_stale() { " (stale)" } else { "" }
            )
        }));
        lines
    }
    /// Shows current pull-request branch and head context.
    fn metadata_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for item in self.pull_request_metadata {
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

        lines
    }
    /// Shows only current chronological evidence; this is not revision history.
    fn timeline_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if !self.timeline.is_empty() {
            lines.push(String::new());
            lines.push("Current timeline:".to_owned());
            for entry in self.timeline {
                let time = entry
                    .occurred_at
                    .map(format_timestamp)
                    .unwrap_or_else(|| "time unavailable".to_owned());
                let summary = timeline_summary(&entry.event);
                lines.push(format!("  {time}: {summary}"));
            }
        }
        lines
    }
}

/// Formats one current source event without controlling surrounding section layout.
fn timeline_summary(event: &ThreadTimelineEvent) -> String {
    match event {
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
    }
}
