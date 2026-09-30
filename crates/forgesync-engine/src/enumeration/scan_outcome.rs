//! # Terminal meaning of a repository acquisition
//!
//! `ScanOutcome` distinguishes a fully traversed scan, caller cancellation, and provider/archive
//! failure. Only complete traversal establishes complete coverage; cancellation remains an
//! interruption without a fabricated provider diagnostic.
//!
//! Traversal selects this outcome and `ScanPersistence` records its coverage and optional failure.
//! The report retains interruption separately so sync accounting can choose its terminal status.
//! Provider errors use the shared safe failure classification; raw responses are never persisted.
//! These mappings describe acquisition, not whether prior observations remain usable.

use forgesync_core::coverage::Failure;
use forgesync_github::error::GitHubError;
use forgesync_store::enumeration::RepositoryThreadScanStatus;

use crate::provider_failure::github_failure;

/// Terminal state of a reserved scan, with failure evidence only when acquisition failed.
pub enum ScanOutcome {
    /// Every page, including the terminal cursor, was durably recorded.
    Complete,
    /// Caller cancellation stopped acquisition without provider failure evidence.
    Interrupted,
    /// Provider or archive failure left acquisition incomplete.
    Failed(Failure),
}
impl From<GitHubError> for ScanOutcome {
    /// Treats provider cancellation as interruption; other errors become safe durable scan
    /// failures.
    fn from(error: GitHubError) -> Self {
        match error {
            GitHubError::Cancelled => Self::Interrupted,
            error => Self::Failed(github_failure(&error)),
        }
    }
}

impl ScanOutcome {
    /// Converts acquisition completion into durable repository coverage.
    pub fn status(&self) -> RepositoryThreadScanStatus {
        match self {
            Self::Complete => RepositoryThreadScanStatus::Complete,
            Self::Interrupted | Self::Failed(_) => RepositoryThreadScanStatus::Incomplete,
        }
    }

    /// Reports caller cancellation separately from provider failure.
    pub fn interrupted(&self) -> bool {
        matches!(self, Self::Interrupted)
    }

    /// Supplies a safe diagnostic only when acquisition failed.
    pub fn failure(&self) -> Option<&Failure> {
        match self {
            Self::Failed(failure) => Some(failure),
            Self::Complete | Self::Interrupted => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use forgesync_core::coverage::FailureKind;
    use forgesync_github::error::GitHubError;
    use forgesync_store::enumeration::RepositoryThreadScanStatus;

    use crate::enumeration::scan_outcome::ScanOutcome;

    #[test]
    fn complete_traversal_has_complete_coverage_without_failure() {
        let outcome = ScanOutcome::Complete;
        assert_eq!(outcome.status(), RepositoryThreadScanStatus::Complete);
        assert!(!outcome.interrupted());
        assert!(outcome.failure().is_none());
    }

    #[test]
    fn cancellation_is_incomplete_without_a_provider_diagnostic() {
        let outcome = ScanOutcome::from(GitHubError::Cancelled);
        assert_eq!(outcome.status(), RepositoryThreadScanStatus::Incomplete);
        assert!(outcome.interrupted());
        assert!(outcome.failure().is_none());
    }

    #[test]
    fn provider_failure_is_incomplete_with_its_safe_classification() {
        let outcome = ScanOutcome::from(GitHubError::Network);
        assert_eq!(outcome.status(), RepositoryThreadScanStatus::Incomplete);
        assert!(!outcome.interrupted());
        assert_eq!(
            outcome.failure().expect("provider diagnostic").kind,
            FailureKind::Network
        );
    }
}
