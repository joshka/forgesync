//! Human thread detail in sections: source, coverage, pull-request context, then timeline.
//!
//! Coverage precedes content that readers might otherwise assume is complete. The timeline
//! describes current evidence, not revision history.

use crate::output::ThreadDetailOutput;
use crate::reports::threads::{discussion_kind_name, format_timestamp, repository_identity};
use crate::reports::timeline::timeline_summary;

/// Formats source content, coverage, pull-request context, and timeline in that order.
pub fn thread_detail_summary(detail: &ThreadDetailOutput<'_>) -> String {
    let mut lines = detail.source_lines();
    lines.extend(detail.coverage_lines());
    lines.extend(detail.metadata_lines());
    lines.extend(detail.timeline_lines());
    lines.join("\n")
}

impl ThreadDetailOutput<'_> {
    /// Source identity, state, and body, independent of acquired child evidence.
    fn source_lines(&self) -> Vec<String> {
        let thread = self.summary.thread;
        let mut lines = vec![format!(
            "{}/{}#{} — {}\nKind: {}\nState: {}\nUpdated: {}",
            self.summary.repository.owner,
            self.summary.repository.name,
            thread.id.number().get(),
            thread.title,
            discussion_kind_name(thread.kind),
            thread.state.as_str(),
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
    /// Per-family coverage, including staleness, before the evidence it qualifies.
    fn coverage_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(String::new());
        lines.push("Coverage:".to_owned());
        lines.extend(self.summary.coverage.iter().map(|coverage| {
            format!(
                "  {}: {}{}",
                coverage.family().as_str(),
                coverage.state().as_str(),
                if coverage.is_stale() { " (stale)" } else { "" }
            )
        }));
        lines
    }
    /// Current pull-request branch and head context, when present.
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
    /// Current chronological evidence; not a revision history.
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
