//! JSON envelopes and command output rendering.
//!
//! The thread and search DTOs rename `discussion` to `thread` and flatten search hits; other
//! command results serialize their engine or store types directly.

use std::io::Write;
use std::process::ExitCode;

use clap::CommandFactory;
use clap::error::ErrorKind;
use forgesync_core::content::{
    Comment, Discussion, PullRequestMetadata, Repository, Review, ReviewThread,
};
use forgesync_core::coverage::Coverage;
use forgesync_engine::inspect::ThreadSort;
use forgesync_engine::search::{SearchMode, SearchProvenance, SearchRanking, SearchResultPage};
use forgesync_store::observations::StagedItem;
use forgesync_store::reads::{
    FamilyCoverageSummary, ThreadDetail, ThreadPage, ThreadSummary, ThreadTimelineEntry,
};
use serde::Serialize;

use crate::command::CliArgs;
use crate::error::{CliError, Exit};

/// Version of the stable Forgesync result envelope.
pub const JSON_SCHEMA_VERSION: u32 = 1;

/// Result presentation selected by the global `--json` flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputMode {
    /// Human summaries on stdout and human errors on stderr.
    Text,
    /// Versioned envelopes on stdout, independent of tracing's stderr encoding.
    Json,
}

impl From<bool> for OutputMode {
    fn from(json: bool) -> Self {
        if json { Self::Json } else { Self::Text }
    }
}

impl OutputMode {
    pub const fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

/// Output mode plus the command path named in JSON envelopes.
#[derive(Clone, Copy, Debug)]
pub struct Output {
    pub mode: OutputMode,
    pub command: &'static str,
}

impl Output {
    pub const fn is_json(self) -> bool {
        self.mode.is_json()
    }

    /// Writes a successful result; see [`Output::report`].
    pub fn success<T: Serialize>(self, data: &T, human: impl FnOnce(&T) -> String) -> Exit {
        self.report(data, human, Exit::Success)
    }

    /// Writes a result to stdout. A closed pipe counts as consumed output, not a failure.
    pub fn report<T: Serialize>(
        self,
        data: &T,
        human: impl FnOnce(&T) -> String,
        exit: Exit,
    ) -> Exit {
        let mut stdout = std::io::stdout().lock();
        let result = if self.is_json() {
            let envelope = JsonEnvelope::success(self.command, data);
            serde_json::to_writer(&mut stdout, &envelope)
                .and_then(|()| writeln!(stdout).map_err(serde_json::Error::io))
        } else {
            writeln!(stdout, "{}", human(data)).map_err(serde_json::Error::io)
        };
        match result {
            Ok(()) => exit,
            Err(error) if error.io_error_kind() == Some(std::io::ErrorKind::BrokenPipe) => exit,
            Err(_) => Exit::Failure,
        }
    }

    /// Writes a failure in the selected format and returns its exit status.
    pub fn error(self, error: &CliError) -> ExitCode {
        if let CliError::Usage(message) = error {
            let error = CliArgs::command().error(ErrorKind::MissingRequiredArgument, message);
            return render_argument_error(&error);
        }
        if self.is_json() {
            let envelope =
                JsonEnvelope::<()>::failure(self.command, error.code(), error.to_string());
            let mut stdout = std::io::stdout().lock();
            if serde_json::to_writer(&mut stdout, &envelope).is_ok() {
                let _ = writeln!(stdout);
            }
        } else {
            let _ = writeln!(std::io::stderr().lock(), "forgesync: {error}");
        }
        error.exit().into()
    }
}

/// Prints Clap's help, version, or usage diagnostic and preserves its exit status.
pub fn render_argument_error(error: &clap::Error) -> ExitCode {
    let _ = error.print();
    ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(2))
}

/// Stable structured error information included in a JSON result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ErrorOutput {
    pub code: String,
    /// Must be free of secrets and private payloads; it is not redacted here.
    pub message: String,
}

/// Versioned process output for successful, partial, or failed operations.
///
/// A success envelope can carry a partial workflow report, so an absent `error` does not mean
/// every work item passed.
#[derive(Clone, Debug, Serialize)]
pub struct JsonEnvelope<T> {
    pub schema_version: u32,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorOutput>,
}

/// One thread with its repository and coverage; `discussion` is published as `thread`.
#[derive(Serialize)]
pub struct ThreadSummaryOutput<'a> {
    pub repository: &'a Repository,
    pub thread: &'a Discussion,
    pub coverage: &'a [Coverage],
}

impl<'a> From<&'a ThreadSummary> for ThreadSummaryOutput<'a> {
    fn from(summary: &'a ThreadSummary) -> Self {
        Self {
            repository: &summary.repository,
            thread: &summary.discussion,
            coverage: &summary.coverage,
        }
    }
}

#[derive(Serialize)]
pub struct ThreadPageOutput<'a> {
    pub items: Vec<ThreadSummaryOutput<'a>>,
    pub next_offset: Option<u64>,
    /// Totals for the selected repository scope, not just the visible items.
    pub coverage: &'a [FamilyCoverageSummary],
}

impl<'a> From<&'a ThreadPage> for ThreadPageOutput<'a> {
    fn from(page: &'a ThreadPage) -> Self {
        Self {
            items: page.items.iter().map(ThreadSummaryOutput::from).collect(),
            next_offset: page.next_offset,
            coverage: &page.coverage,
        }
    }
}

/// Search results; unlike the engine page, `fallback_reason` is always present (null when unused).
#[derive(Serialize)]
pub struct SearchPageOutput<'a> {
    pub query: &'a str,
    pub requested_mode: SearchMode,
    pub mode: SearchMode,
    pub ranking: SearchRanking,
    pub sort: ThreadSort,
    pub fallback_reason: Option<&'a str>,
    pub items: Vec<SearchHitOutput<'a>>,
    pub next_offset: Option<u64>,
    pub coverage: &'a [FamilyCoverageSummary],
}

/// One ranked result with the thread summary fields at the top level.
#[derive(Serialize)]
pub struct SearchHitOutput<'a> {
    pub repository: &'a Repository,
    pub thread: &'a Discussion,
    pub coverage: &'a [Coverage],
    /// Omitted for keyword-only ranking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    pub provenance: &'a [SearchProvenance],
}

impl<'a> From<&'a SearchResultPage> for SearchPageOutput<'a> {
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

/// Thread detail; retained child members may be stale, as reported by summary coverage.
#[derive(Serialize)]
pub struct ThreadDetailOutput<'a> {
    pub summary: ThreadSummaryOutput<'a>,
    pub comments: &'a [StagedItem<Comment>],
    pub pull_request_metadata: &'a [StagedItem<PullRequestMetadata>],
    pub reviews: &'a [StagedItem<Review>],
    pub review_threads: &'a [StagedItem<ReviewThread>],
    pub timeline: &'a [ThreadTimelineEntry],
}

impl<'a> From<&'a ThreadDetail> for ThreadDetailOutput<'a> {
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
    pub fn success(command: impl Into<String>, data: T) -> Self {
        Self {
            schema_version: JSON_SCHEMA_VERSION,
            command: command.into(),
            data: Some(data),
            warnings: Vec::new(),
            error: None,
        }
    }

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
