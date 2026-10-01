//! Thread-list and search page summaries.
//!
//! Coverage describes the selected repository scope, not completeness inferred from the visible
//! rows. Search rank is one-based within the page.

use forgesync_store::reads::FamilyCoverageSummary;

use crate::output::{SearchHitOutput, SearchPageOutput, ThreadPageOutput, ThreadSummaryOutput};
use crate::reports::threads::discussion_kind_name;

impl ThreadPageOutput<'_> {
    pub fn summary(&self) -> String {
        let mut lines = vec!["REPOSITORY\tNUMBER\tKIND\tSTATE\tTITLE".to_owned()];
        lines.extend(self.items.iter().map(ThreadSummaryOutput::table_row));
        if self.items.is_empty() {
            lines.push("No discussions matched.".to_owned());
        }
        lines.extend(page_footer(self.coverage, self.next_offset));
        lines.join("\n")
    }
}

impl ThreadSummaryOutput<'_> {
    fn table_row(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}",
            self.repository.full_name,
            self.thread.id.number().get(),
            discussion_kind_name(self.thread.kind),
            self.thread.state.as_str(),
            self.thread.title
        )
    }
}

impl SearchPageOutput<'_> {
    pub fn summary(&self) -> String {
        let mut lines = vec![format!(
            "Mode: {:?} (requested {:?}), ranking: {:?}, sort: {:?}",
            self.mode, self.requested_mode, self.ranking, self.sort
        )];
        if let Some(reason) = self.fallback_reason {
            lines.push(format!("Keyword fallback: {reason}"));
        }
        lines.push("RANK\tSCORE\tREPOSITORY\tNUMBER\tKIND\tSTATE\tTITLE".to_owned());
        lines.extend(
            self.items
                .iter()
                .enumerate()
                .map(|(index, item)| item.ranked_row(index + 1)),
        );
        if self.items.is_empty() {
            lines.push("No discussions matched.".to_owned());
        }
        lines.extend(page_footer(self.coverage, self.next_offset));
        lines.join("\n")
    }
}

impl SearchHitOutput<'_> {
    /// Formats a one-based page rank and a six-decimal score, or `-` when absent.
    fn ranked_row(&self, rank: usize) -> String {
        let score = self
            .score
            .map(|score| format!("{score:.6}"))
            .unwrap_or_else(|| "-".to_owned());
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            rank,
            score,
            self.repository.full_name,
            self.thread.id.number().get(),
            discussion_kind_name(self.thread.kind),
            self.thread.state.as_str(),
            self.thread.title
        )
    }
}

/// Appends scope-wide coverage and the next offset after either result table.
fn page_footer(coverage: &[FamilyCoverageSummary], next_offset: Option<u64>) -> Vec<String> {
    let mut lines = vec!["Coverage:".to_owned()];
    lines.extend(coverage.iter().map(coverage_line));
    if let Some(next_offset) = next_offset {
        lines.push(format!("Next offset: {next_offset}"));
    }
    lines
}

/// One family's complete, incomplete, and missing counts out of applicable threads.
pub(super) fn coverage_line(coverage: &FamilyCoverageSummary) -> String {
    format!(
        "  {}: {} complete, {} incomplete, {} missing of {}",
        coverage.family.as_str(),
        coverage.complete,
        coverage.incomplete,
        coverage.missing,
        coverage.applicable_threads
    )
}

#[cfg(test)]
mod tests {
    use forgesync_core::coverage::EvidenceFamily;
    use forgesync_store::reads::FamilyCoverageSummary;

    use crate::output::ThreadPageOutput;
    use crate::reports::pages::page_footer;

    #[test]
    fn empty_thread_page_keeps_header_message_and_coverage_heading() {
        let page = ThreadPageOutput {
            items: Vec::new(),
            next_offset: None,
            coverage: &[],
        };
        assert_eq!(
            page.summary(),
            "REPOSITORY\tNUMBER\tKIND\tSTATE\tTITLE\nNo discussions matched.\nCoverage:"
        );
    }

    #[test]
    fn coverage_footer_keeps_distinct_counts_before_next_offset() {
        let coverage = FamilyCoverageSummary {
            family: EvidenceFamily::Comments,
            complete: 3,
            incomplete: 2,
            missing: 1,
            applicable_threads: 6,
        };
        assert_eq!(
            page_footer(&[coverage], Some(20)),
            vec![
                "Coverage:",
                "  comments: 3 complete, 2 incomplete, 1 missing of 6",
                "Next offset: 20",
            ]
        );
    }
}
