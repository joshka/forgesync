//! # JSON contracts for command results
//!
//! `JsonEnvelope` and the output projection types define what machine-readable CLI users receive.
//! They adapt engine and store reports into command-oriented fields while preserving status,
//! coverage, and partial failure information.
//!
//! These shapes are distinct from provider DTOs, domain types, and SQL rows. Human summaries live
//! in `reports`; command handlers choose the mode. Keep projection meaning independent of terminal
//! wording, with command-specific DTOs at their owning command when appropriate.
//!
//! [`JsonEnvelope`] provides schema version, command path, optional data/error, and always-present
//! warnings. Its constructors establish ordinary success/failure shapes but do not serialize,
//! print, choose exit codes, or sanitize messages. A successful envelope can contain a structured
//! partial workflow report; absence of its top-level error is not proof that every work item
//! passed.
//!
//! Archive projections borrow metadata, counts, coverage, and diagnostics. Thread projections
//! preserve normalized parent content alongside independently acquired child families. Retained
//! last-complete members can remain visible while coverage reports them stale; these DTOs do not
//! filter or reacquire evidence, and they inherit the store read's consistency limits.
//!
//! Search projections retain requested/effective modes, ranking, fallback reason, and provenance.
//! Keyword scores are omitted rather than serialized as null, while continuation and fallback
//! option fields retain their own declared serialization shape. Conversion allocates row wrappers
//! where needed but otherwise borrows loaded values, without archive or network effects.
//!
//! Callers must supply safe error text: [`ErrorOutput`] is a string record, not a redaction
//! service. Rendering and exit-code policy live at the command/report boundary. Argument-parser
//! failures occur before these application-result constructors.

use forgesync_core::content::{
    Comment, Discussion, PullRequestMetadata, Repository, Review, ReviewThread,
};
use forgesync_core::coverage::Coverage;
use forgesync_engine::inspect::ThreadSort;
use forgesync_engine::search::{SearchMode, SearchProvenance, SearchRanking, SearchResultPage};
use forgesync_store::archive::ArchiveInfo;
use forgesync_store::diagnostics::ArchiveDiagnostics;
use forgesync_store::observations::StagedItem;
use forgesync_store::reads::{
    ArchiveStatus, FamilyCoverageSummary, ThreadDetail, ThreadPage, ThreadSummary,
    ThreadTimelineEntry,
};
use serde::Serialize;

/// Version of the stable Forgesync result envelope.
pub const JSON_SCHEMA_VERSION: u32 = 1;

/// Stable structured error information included in a JSON result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ErrorOutput {
    /// Machine-readable error classification.
    pub code: String,
    /// Human-readable summary that the caller must keep free of secrets/private payloads.
    /// Construction and serialization do not redact this string.
    pub message: String,
}

/// Versioned process output for successful, partial, or failed application operations.
#[derive(Clone, Debug, Serialize)]
pub struct JsonEnvelope<T> {
    /// Version of this output contract.
    pub schema_version: u32,
    /// Stable command path that produced this result.
    pub command: String,
    /// Command result, omitted when startup or execution failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// Non-fatal, actionable notes.
    pub warnings: Vec<String>,
    /// Structured failure, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorOutput>,
}

/// Stable JSON view of archive metadata and local record counts.
#[derive(Serialize)]
pub struct ArchiveStatusOutput<'a> {
    /// Validated archive lifecycle metadata.
    pub archive: &'a ArchiveInfo,
    /// Registered repository count.
    pub repositories: u64,
    /// Archived discussion count.
    pub threads: u64,
    /// Archived issue count.
    pub issues: u64,
    /// Archived pull-request count.
    pub pull_requests: u64,
    /// Complete, incomplete, and missing counts by family.
    pub coverage: &'a [FamilyCoverageSummary],
    /// Schema, lease, and durable-work diagnostics.
    pub diagnostics: &'a ArchiveDiagnostics,
}

impl<'a> From<&'a ArchiveStatus> for ArchiveStatusOutput<'a> {
    /// Borrows archive metadata, coverage, and diagnostics while copying counts into command JSON.
    /// Conversion performs no archive lifecycle or health operation.
    fn from(status: &'a ArchiveStatus) -> Self {
        Self {
            archive: &status.archive,
            repositories: status.repositories,
            threads: status.threads,
            issues: status.issues,
            pull_requests: status.pull_requests,
            coverage: &status.coverage,
            diagnostics: &status.diagnostics,
        }
    }
}

/// Stable JSON view of one thread, its repository, and current family coverage.
#[derive(Serialize)]
pub struct ThreadSummaryOutput<'a> {
    /// Current repository identity and display metadata.
    pub repository: &'a Repository,
    /// Current normalized source discussion.
    pub thread: &'a Discussion,
    /// Coverage for the thread's applicable evidence families.
    pub coverage: &'a [Coverage],
}

impl<'a> From<&'a ThreadSummary> for ThreadSummaryOutput<'a> {
    /// Borrows repository, discussion, and coverage under the command schema's `thread` field.
    /// The store projection remains separate from this public JSON shape.
    fn from(summary: &'a ThreadSummary) -> Self {
        Self {
            repository: &summary.repository,
            thread: &summary.discussion,
            coverage: &summary.coverage,
        }
    }
}

/// Stable JSON view of a thread-list or search page.
#[derive(Serialize)]
pub struct ThreadPageOutput<'a> {
    /// Thread results in stable query order.
    pub items: Vec<ThreadSummaryOutput<'a>>,
    /// Offset for the next page, when more results are available.
    pub next_offset: Option<u64>,
    /// Coverage totals for the selected repository scope.
    pub coverage: &'a [FamilyCoverageSummary],
}

impl<'a> From<&'a ThreadPage> for ThreadPageOutput<'a> {
    /// Adapts page rows while preserving their continuation and aggregate coverage.
    /// Coverage remains the query projection rather than being inferred from these visible items.
    fn from(page: &'a ThreadPage) -> Self {
        Self {
            items: page.items.iter().map(ThreadSummaryOutput::from).collect(),
            next_offset: page.next_offset,
            coverage: &page.coverage,
        }
    }
}

/// Stable JSON view of keyword, semantic, and hybrid search results.
#[derive(Serialize)]
pub struct SearchPageOutput<'a> {
    /// Query text retained by the engine after its boundary trimming.
    pub query: &'a str,
    /// Mode requested by the caller.
    pub requested_mode: SearchMode,
    /// Mode that produced the returned results.
    pub mode: SearchMode,
    /// Effective ranking policy.
    pub ranking: SearchRanking,
    /// Search sort order applied to the results.
    pub sort: ThreadSort,
    /// Provider or vector-coverage classification when explicit fallback was used.
    pub fallback_reason: Option<&'a str>,
    /// Ranked discussions and per-result source evidence.
    pub items: Vec<SearchHitOutput<'a>>,
    /// Offset to pass to the next request, when more results are available.
    pub next_offset: Option<u64>,
    /// Coverage totals for the selected repository scope.
    pub coverage: &'a [FamilyCoverageSummary],
}

/// Stable view of one result while preserving the existing top-level thread fields.
#[derive(Serialize)]
pub struct SearchHitOutput<'a> {
    /// Current repository identity and display metadata.
    pub repository: &'a Repository,
    /// Current normalized source discussion.
    pub thread: &'a Discussion,
    /// Current evidence coverage.
    pub coverage: &'a [Coverage],
    /// Mode-specific score, omitted for keyword-only ranking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Source ranks used to produce the result.
    pub provenance: &'a [SearchProvenance],
}

impl<'a> From<&'a SearchResultPage> for SearchPageOutput<'a> {
    /// Borrows ranked rows and provenance while preserving requested/effective modes and fallback.
    /// Conversion performs no search or embedding-service request.
    fn from(page: &'a SearchResultPage) -> Self {
        Self {
            query: &page.query,
            requested_mode: page.requested_mode,
            mode: page.mode,
            ranking: page.ranking,
            sort: page.sort,
            fallback_reason: page.fallback_reason.as_deref(),
            items: page
                .items
                .iter()
                .map(|hit| SearchHitOutput {
                    repository: &hit.summary.repository,
                    thread: &hit.summary.discussion,
                    coverage: &hit.summary.coverage,
                    score: hit.score,
                    provenance: &hit.provenance,
                })
                .collect(),
            next_offset: page.next_offset,
            coverage: &page.coverage,
        }
    }
}

/// Stable JSON view of current thread content and selected child-family evidence.
#[derive(Serialize)]
pub struct ThreadDetailOutput<'a> {
    /// Discussion summary and per-family coverage.
    pub summary: ThreadSummaryOutput<'a>,
    /// Retained comment membership; inspect summary coverage for completeness and staleness.
    pub comments: &'a [StagedItem<Comment>],
    /// Current pull-request metadata.
    pub pull_request_metadata: &'a [StagedItem<PullRequestMetadata>],
    /// Retained review membership, which can describe an earlier head when marked stale.
    pub reviews: &'a [StagedItem<Review>],
    /// Retained review-thread membership with completeness/head freshness in summary coverage.
    pub review_threads: &'a [StagedItem<ReviewThread>],
    /// Chronological projection of current source content.
    pub timeline: &'a [ThreadTimelineEntry],
}

impl<'a> From<&'a ThreadDetail> for ThreadDetailOutput<'a> {
    /// Keeps parent summary and independently acquired child families together in command JSON.
    /// Content and timeline are borrowed from the archived detail rather than reacquired or
    /// flattened.
    fn from(detail: &'a ThreadDetail) -> Self {
        Self {
            summary: ThreadSummaryOutput::from(&detail.summary),
            comments: &detail.comments,
            pull_request_metadata: &detail.pull_request_metadata,
            reviews: &detail.reviews,
            review_threads: &detail.review_threads,
            timeline: &detail.timeline,
        }
    }
}

impl<T> JsonEnvelope<T> {
    /// Wraps supplied data with the current schema version and no top-level error.
    ///
    /// Starts with empty warnings. Data is accepted unchanged, including structured partial
    /// reports; this constructor does not validate workflow success, print JSON, or select an
    /// exit code.
    pub fn success(command: impl Into<String>, data: T) -> Self {
        Self {
            schema_version: JSON_SCHEMA_VERSION,
            command: command.into(),
            data: Some(data),
            warnings: Vec::new(),
            error: None,
        }
    }

    /// Wraps an application failure with no data and initially empty warnings.
    ///
    /// The caller supplies a stable classification and safe human message. Values are retained
    /// verbatim without redaction or validation. Argument parsing, serialization, output streams,
    /// and process exit policy remain outside this constructor.
    pub fn failure(
        command: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: JSON_SCHEMA_VERSION,
            command: command.into(),
            data: None,
            warnings: Vec::new(),
            error: Some(ErrorOutput {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Versioned envelope serialization at the application output boundary.
    //!
    //! These direct examples establish optional data/error omission and always-present warnings.
    //! They serialize synthetic values without invoking commands or inferring exit status from
    //! the envelope. Command integration tests cover actual rendering and process behavior.
    //!
    //! Whole-value comparisons establish field names and omission rules for both outcomes.
    //! The supplied command, code, and message are boundary facts; serialization neither validates
    //! a domain operation nor sanitizes arbitrary text. No stream write or process runs here.

    use serde_json::json;

    use crate::output::JsonEnvelope;

    #[test]
    fn failure_uses_versioned_envelope_without_data() {
        let envelope = JsonEnvelope::<serde_json::Value>::failure(
            "archive status",
            "archive_missing",
            "archive path does not exist",
        );
        let value = serde_json::to_value(envelope).expect("envelope should serialize");

        assert_eq!(
            value,
            json!({
                "schema_version": 1,
                "command": "archive status",
                "warnings": [],
                "error": {
                    "code": "archive_missing",
                    "message": "archive path does not exist"
                }
            })
        );
    }

    #[test]
    fn success_includes_data_and_empty_warnings() {
        let envelope = JsonEnvelope::success("archive status", json!({ "repositories": 2 }));
        let value = serde_json::to_value(envelope).expect("envelope should serialize");

        assert_eq!(
            value,
            json!({
                "schema_version": 1,
                "command": "archive status",
                "warnings": [],
                "data": { "repositories": 2 }
            })
        );
    }
}
