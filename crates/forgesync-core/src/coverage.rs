//! Evidence-family coverage and failures for partial acquisition.
//!
//! A complete empty collection is evidence that no members exist. A missing or incomplete
//! collection cannot delete previously complete membership, so consumers inspect coverage rather
//! than infer it from an empty vector of comments or reviews. Staleness is separate: retained
//! complete evidence can belong to an earlier parent context.

use serde::{Deserialize, Serialize};

use crate::identity::ObservationSequence;
use crate::observation::IncompleteReason;
use crate::timestamp::UtcTimestamp;

/// A selected family of source evidence tracked independently in coverage.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceFamily {
    Threads,
    Comments,
    PullRequestMetadata,
    Reviews,
    /// Current pull request review threads and their comments.
    ReviewThreads,
}

impl EvidenceFamily {
    /// Returns the serde and archive spelling, such as `pull_request_metadata`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Threads => "threads",
            Self::Comments => "comments",
            Self::PullRequestMetadata => "pull_request_metadata",
            Self::Reviews => "reviews",
            Self::ReviewThreads => "review_threads",
        }
    }
}

/// Stable failure category retained in structured operation reports; not a retry policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// No usable credential was supplied.
    Authentication,
    /// The credential cannot read this resource.
    PermissionDenied,
    /// Provider throttling exhausted the configured wait budget.
    RateLimited,
    /// The request failed before receiving a usable response.
    Network,
    /// The provider returned an unusable or inconsistent response.
    ProviderResponse,
    /// The local archive could not commit or read the operation.
    Archive,
    /// Normalized source data violated a domain constraint.
    InvalidData,
}

/// Durable failure summary for terminal output and archive reporting.
///
/// Consumers choose recovery from `kind`, never by parsing `message`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Failure {
    pub kind: FailureKind,
    /// Producers must exclude credentials and raw payloads; nothing here redacts them.
    pub message: String,
}

/// Reason why work was intentionally left for a later run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeferredReason {
    /// The configured retry budget cannot accommodate the provider wait.
    RateLimitBudget,
    /// The caller selected an offline operation.
    Offline,
}

/// Completeness and freshness state for one evidence family.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CoverageState {
    /// No successful observation or classified attempt exists yet.
    Missing,
    /// Some data was acquired, but the collection did not finish.
    Incomplete {
        observed_at: UtcTimestamp,
        sequence: ObservationSequence,
        reason: IncompleteReason,
        received_items: u64,
        failure: Option<Failure>,
    },
    /// A full collection completed, including a valid empty result.
    Complete {
        observed_at: UtcTimestamp,
        sequence: ObservationSequence,
        /// Zero means complete empty.
        item_count: u64,
    },
}

impl CoverageState {
    /// Returns the serde status tag of this state.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Incomplete { .. } => "incomplete",
            Self::Complete { .. } => "complete",
        }
    }
}

/// Per-family coverage state for a repository or discussion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    family: EvidenceFamily,
    state: CoverageState,
    /// Retained evidence belongs to an earlier parent context; omitted from JSON when false.
    #[serde(default, skip_serializing_if = "is_not_stale")]
    stale: bool,
}

impl Coverage {
    /// Creates fresh (not stale) coverage for one evidence family.
    ///
    /// ```
    /// use forgesync_core::coverage::{Coverage, CoverageState, EvidenceFamily};
    ///
    /// let coverage = Coverage::new(EvidenceFamily::Comments, CoverageState::Missing);
    /// assert!(!coverage.is_stale());
    /// let stale = coverage.mark_stale();
    /// assert!(stale.is_stale());
    /// assert_eq!(stale.state(), &CoverageState::Missing);
    /// ```
    pub fn new(family: EvidenceFamily, state: CoverageState) -> Self {
        Self {
            family,
            state,
            stale: false,
        }
    }

    /// Marks this evidence as belonging to an older parent context, independent of completeness.
    pub fn mark_stale(mut self) -> Self {
        self.stale = true;
        self
    }

    pub fn family(&self) -> EvidenceFamily {
        self.family
    }

    pub fn state(&self) -> &CoverageState {
        &self.state
    }

    pub fn is_stale(&self) -> bool {
        self.stale
    }
}

/// Omits the false stale marker from serialized coverage, retaining the legacy fresh projection.
fn is_not_stale(stale: &bool) -> bool {
    !stale
}

#[cfg(test)]
mod tests {
    //! Coverage JSON representation: complete-empty versus missing, and staleness.

    use serde_json::json;

    use crate::coverage::{Coverage, CoverageState, EvidenceFamily};
    use crate::identity::ObservationSequence;
    use crate::timestamp::UtcTimestamp;

    #[test]
    fn family_names_match_serde_spelling() {
        for family in [
            EvidenceFamily::Threads,
            EvidenceFamily::Comments,
            EvidenceFamily::PullRequestMetadata,
            EvidenceFamily::Reviews,
            EvidenceFamily::ReviewThreads,
        ] {
            assert_eq!(serde_json::to_value(family).unwrap(), family.as_str());
        }
    }

    #[test]
    fn complete_empty_coverage_is_distinct_from_missing_coverage() {
        let observed_at = UtcTimestamp::parse("2026-09-20T10:00:00Z").expect("timestamp");
        let sequence = ObservationSequence::new(3).expect("sequence");
        let complete = Coverage::new(
            EvidenceFamily::Comments,
            CoverageState::Complete {
                observed_at,
                sequence,
                item_count: 0,
            },
        );
        let missing = Coverage::new(EvidenceFamily::Comments, CoverageState::Missing);

        let complete_json = serde_json::to_value(complete).expect("serialize complete coverage");
        let missing_json = serde_json::to_value(missing).expect("serialize missing coverage");
        assert_eq!(
            complete_json,
            json!({
                "family": "comments",
                "state": {
                    "status": "complete",
                    "observed_at": "2026-09-20T10:00:00Z",
                    "sequence": 3,
                    "item_count": 0
                }
            })
        );
        assert_eq!(
            missing_json,
            json!({ "family": "comments", "state": { "status": "missing" } })
        );
    }

    #[test]
    fn stale_coverage_is_exposed_separately_from_completeness() {
        let observed_at = UtcTimestamp::parse("2026-09-20T10:00:00Z").expect("timestamp");
        let sequence = ObservationSequence::new(3).expect("sequence");
        let stale = Coverage::new(
            EvidenceFamily::Comments,
            CoverageState::Complete {
                observed_at,
                sequence,
                item_count: 2,
            },
        );
        let stale = stale.mark_stale();

        let stale_json = serde_json::to_value(stale).expect("serialize stale coverage");
        assert_eq!(
            stale_json,
            json!({
                "family": "comments",
                "state": {
                    "status": "complete",
                    "observed_at": "2026-09-20T10:00:00Z",
                    "sequence": 3,
                    "item_count": 2
                },
                "stale": true
            })
        );
    }
}
