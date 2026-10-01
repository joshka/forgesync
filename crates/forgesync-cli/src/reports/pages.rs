//! # Present discussion and ranked-search pages
//!
//! Human summaries belong to the existing page and result output types. `ThreadPageOutput` shows
//! source rows; `SearchPageOutput` adds effective retrieval policy, fallback, rank, and score. The
//! same DTOs retain their existing JSON representation, with no extra presentation model.
//!
//! Row methods own field formatting. Page methods own headings, empty-result messages, and order.
//! Both append the same aggregate coverage and continuation footer. Coverage describes the selected
//! repository scope, rather than completeness inferred from the visible page's result count.
//!
//! Result order comes from the engine/store. Displayed search rank is one-based within this page,
//! scores use six decimal places, and absent scores use a dash. These methods perform no query,
//! mutate no pagination state, and preserve source title text as supplied.

use forgesync_store::reads::FamilyCoverageSummary;

use crate::output::{SearchHitOutput, SearchPageOutput, ThreadPageOutput, ThreadSummaryOutput};
use crate::reports::threads::{discussion_kind_name, family_name, source_state_name};

impl ThreadPageOutput<'_> {
    /// Presents source rows followed by aggregate evidence and the optional next offset.
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
    /// Shows repository, source identity, kind, state, and title without inventing retrieval
    /// scores.
    fn table_row(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}",
            self.repository.full_name,
            self.thread.id.number().get(),
            discussion_kind_name(self.thread.kind),
            source_state_name(&self.thread.state),
            self.thread.title
        )
    }
}

impl SearchPageOutput<'_> {
    /// Presents effective retrieval policy and page-relative ranking without altering result order.
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
    /// Shows a caller-supplied one-based display rank and an optional six-decimal retrieval score.
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
            source_state_name(&self.thread.state),
            self.thread.title
        )
    }
}

/// Appends scope-wide family counts and continuation after either kind of result table.
fn page_footer(coverage: &[FamilyCoverageSummary], next_offset: Option<u64>) -> Vec<String> {
    let mut lines = vec!["Coverage:".to_owned()];
    lines.extend(coverage.iter().map(coverage_line));
    if let Some(next_offset) = next_offset {
        lines.push(format!("Next offset: {next_offset}"));
    }
    lines
}

/// Keeps complete, incomplete, missing, and applicable counts distinct for one evidence family.
pub(super) fn coverage_line(coverage: &FamilyCoverageSummary) -> String {
    format!(
        "  {}: {} complete, {} incomplete, {} missing of {}",
        family_name(coverage.family),
        coverage.complete,
        coverage.incomplete,
        coverage.missing,
        coverage.applicable_threads
    )
}

#[cfg(test)]
mod tests {
    //! # Empty-page and coverage-footer presentation
    //!
    //! These cases build public page/coverage projections directly without archive queries.
    //! Empty results retain the table header and coverage heading, distinguishing no matches
    //! from a missing report. Footer output keeps complete, incomplete, and missing counts
    //! separate.
    //!
    //! The continuation line follows coverage and uses the supplied engine offset unchanged.
    //! Counts are fixture facts; formatting does not derive or validate source completeness.
    //! Exact strings/lines establish ordering and wording rather than terminal layout dimensions.
    //! Process cases separately cover query selection and output delivery.

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
