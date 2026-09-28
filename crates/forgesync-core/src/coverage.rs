use serde::{Deserialize, Serialize};

use crate::{IncompleteReason, ObservationSequence, UtcTimestamp};

/// A selected family of source evidence tracked independently in coverage.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceFamily {
    /// Repository thread listing and issue/PR metadata.
    Threads,
    /// Discussion comments.
    Comments,
    /// Pull request base/head metadata.
    PullRequestMetadata,
    /// Pull request reviews.
    Reviews,
    /// Current pull request review threads and their comments.
    ReviewThreads,
}

/// Failure classification safe to keep in structured operation reports.
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
    /// Another writer or expired lease prevented this operation from continuing.
    LeaseLost,
    /// Normalized source data violated a domain constraint.
    InvalidData,
}

/// Safe, durable summary of a failed unit of work.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Failure {
    /// Stable machine-readable failure class.
    pub kind: FailureKind,
    /// Human-readable summary without tokens or raw provider payloads.
    pub message: String,
}

/// Reason why a selected family is inaccessible for the current archive identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    /// The current credential is not authorized to read the family.
    PermissionDenied,
    /// The selected family does not apply to this source resource.
    NotApplicable,
    /// The provider does not expose the requested evidence.
    ProviderUnsupported,
}

/// Reason why work was intentionally left for a later run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeferredReason {
    /// The configured retry budget cannot accommodate the provider wait.
    RateLimitBudget,
    /// The caller selected an offline operation.
    Offline,
    /// The user did not select this optional family.
    NotSelected,
    /// The work is outside the selected archive scope.
    OutOfScope,
}

/// Completeness and freshness state for one evidence family.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CoverageState {
    /// No successful observation or classified attempt exists yet.
    Missing,
    /// Some data was acquired, but the collection did not finish.
    Incomplete {
        /// Time when this collection attempt was acquired.
        observed_at: UtcTimestamp,
        /// Acquisition sequence reserved before the request.
        sequence: ObservationSequence,
        /// Why acquisition did not finish the requested collection.
        reason: IncompleteReason,
        /// Number of items safely received before the incomplete result.
        received_items: u64,
        /// Failure that stopped collection, when known.
        failure: Option<Failure>,
    },
    /// A full collection completed, including a valid empty result.
    Complete {
        /// Time when this complete collection was acquired.
        observed_at: UtcTimestamp,
        /// Acquisition sequence reserved before the request.
        sequence: ObservationSequence,
        /// Number of items in the complete collection; zero means complete empty.
        item_count: u64,
    },
    /// The source was contacted but the family could not be read.
    Unavailable {
        /// Time when this unavailable result was observed.
        observed_at: UtcTimestamp,
        /// Acquisition sequence reserved before the request.
        sequence: ObservationSequence,
        /// Classified reason for the unavailable evidence.
        reason: UnavailableReason,
    },
    /// An attempted read failed without producing a commit-ready collection.
    Failed {
        /// Time when the failed attempt was observed.
        observed_at: UtcTimestamp,
        /// Acquisition sequence reserved before the request.
        sequence: ObservationSequence,
        /// Safe failure summary.
        failure: Failure,
    },
    /// The operation intentionally did not attempt this family.
    Deferred {
        /// Reason the family remains pending.
        reason: DeferredReason,
    },
}

/// Per-family coverage state for a repository or discussion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    family: EvidenceFamily,
    state: CoverageState,
    #[serde(default, skip_serializing_if = "is_not_stale")]
    stale: bool,
}

impl Coverage {
    /// Creates coverage for a single independently acquired evidence family.
    pub fn new(family: EvidenceFamily, state: CoverageState) -> Self {
        Self {
            family,
            state,
            stale: false,
        }
    }

    /// Marks complete or incomplete evidence as belonging to an older parent observation.
    pub fn with_stale(mut self, stale: bool) -> Self {
        self.stale = stale;
        self
    }

    /// Returns the evidence family this coverage describes.
    pub fn family(&self) -> EvidenceFamily {
        self.family
    }

    /// Returns the current state for this family.
    pub fn state(&self) -> &CoverageState {
        &self.state
    }

    /// Returns whether this family's evidence predates its current parent or interpretation context.
    pub fn is_stale(&self) -> bool {
        self.stale
    }
}

fn is_not_stale(stale: &bool) -> bool {
    !stale
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{ObservationSequence, UtcTimestamp};

    use super::{Coverage, CoverageState, EvidenceFamily};

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
        let missing = Coverage::new(EvidenceFamily::Reviews, CoverageState::Missing);

        let complete_json = serde_json::to_value(complete).expect("serialize complete coverage");
        let missing_json = serde_json::to_value(missing).expect("serialize missing coverage");
        assert_eq!(complete_json["state"]["status"], json!("complete"));
        assert_eq!(complete_json["state"]["item_count"], json!(0));
        assert_eq!(missing_json["state"]["status"], json!("missing"));
        assert_ne!(complete_json, missing_json);
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
        )
        .with_stale(true);

        let stale_json = serde_json::to_value(stale).expect("serialize stale coverage");
        assert_eq!(stale_json["state"]["status"], json!("complete"));
        assert_eq!(stale_json["stale"], json!(true));
    }
}
