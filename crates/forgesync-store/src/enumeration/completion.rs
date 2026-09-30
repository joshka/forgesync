//! # Validate and persist terminal repository coverage
//!
//! `ScanCompletion` represents the only terminal states accepted by the scan writer. Complete
//! coverage cannot carry a failure; incomplete coverage may retain a safe diagnostic or represent
//! interruption without one. Construction rejects invalid public status/failure combinations
//! before a transaction begins.
//!
//! The archive coordinator opens and fences the transaction. `validate_cursor` requires the exact
//! active acquisition generation and rejects completion while a next-page cursor remains. `write`
//! updates that same generation and verifies one row changed before the caller commits.
//!
//! Incomplete finalization retains the cursor for diagnostics; complete finalization clears it.
//! This evidence is repository enumeration coverage, independent of individual thread families.

use forgesync_core::coverage::Failure;
use forgesync_core::identity::RepositoryId;
use forgesync_core::timestamp::UtcTimestamp;
use sqlx::{Row, SqliteConnection};

use crate::enumeration::RepositoryThreadScanStatus;
use crate::error::StoreError;

/// Validated terminal state, with diagnostics confined to incomplete acquisition.
#[derive(Debug)]
pub enum ScanCompletion<'a> {
    /// All page cursors have been committed and no failure may be attached.
    Complete,
    /// Acquisition stopped early, optionally with safe failure evidence.
    Incomplete(Option<&'a Failure>),
}

impl<'a> ScanCompletion<'a> {
    /// Rejects active status and failure-bearing completion before any archive mutation.
    pub fn new(
        status: RepositoryThreadScanStatus,
        failure: Option<&'a Failure>,
    ) -> Result<Self, StoreError> {
        match (status, failure) {
            (RepositoryThreadScanStatus::Complete, None) => Ok(Self::Complete),
            (RepositoryThreadScanStatus::Incomplete, failure) => Ok(Self::Incomplete(failure)),
            _ => Err(StoreError::InvalidRepositoryThreadScan),
        }
    }

    /// Requires an active matching generation and a terminal cursor for complete coverage.
    pub async fn validate_cursor(
        &self,
        connection: &mut SqliteConnection,
        repository: &RepositoryId,
        sequence: i64,
    ) -> Result<(), StoreError> {
        let row = sqlx::query(
            "SELECT next_page_url FROM repository_thread_scans WHERE repository_id = (SELECT id FROM repositories WHERE host = ? AND provider_id = ?) AND sequence = ? AND status = 'in_progress'",
        )
        .bind(repository.host().as_str())
        .bind(repository.provider_id().as_str())
        .bind(sequence)
        .fetch_optional(connection)
        .await?
        .ok_or(StoreError::RepositoryThreadScanMissing)?;
        let next_page_url: Option<String> = row.try_get("next_page_url")?;
        if matches!(self, Self::Complete) && next_page_url.is_some() {
            return Err(StoreError::InvalidRepositoryThreadScan);
        }
        Ok(())
    }

    /// Writes validated terminal evidence without advancing the page checkpoint.
    pub async fn write(
        &self,
        connection: &mut SqliteConnection,
        repository: &RepositoryId,
        sequence: i64,
        updated_at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        let status_name = self.status_name();
        let failure_json = self.failure().map(serde_json::to_string).transpose()?;
        let result = sqlx::query(
            "UPDATE repository_thread_scans SET status = ?, updated_at_us = ?, next_page_url = CASE WHEN ? = 'complete' THEN NULL ELSE next_page_url END, failure_json = ? WHERE repository_id = (SELECT id FROM repositories WHERE host = ? AND provider_id = ?) AND sequence = ? AND status = 'in_progress'",
        )
        .bind(status_name)
        .bind(updated_at.unix_microseconds())
        .bind(status_name)
        .bind(failure_json)
        .bind(repository.host().as_str())
        .bind(repository.provider_id().as_str())
        .bind(sequence)
        .execute(connection)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::RepositoryThreadScanMissing);
        }
        Ok(())
    }

    /// Returns the database spelling of this validated terminal state.
    fn status_name(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Incomplete(_) => "incomplete",
        }
    }

    /// Supplies optional safe evidence for an incomplete terminal write.
    fn failure(&self) -> Option<&Failure> {
        match self {
            Self::Complete => None,
            Self::Incomplete(failure) => *failure,
        }
    }
}

#[cfg(test)]
mod tests {
    //! # Terminal scan declaration validation
    //!
    //! These cases construct completion declarations directly from status and diagnostic evidence.
    //! An active scan cannot finalize, and complete coverage cannot carry a provider failure.
    //! Complete success and incomplete interruption remain valid declarations without diagnostics.
    //! Each rejected declaration checks the specific invalid-scan error variant.
    //!
    //! No scan ledger or archive is created here. Integration tests own durable finalization,
    //! checkpoint publication, membership, and lease fencing. This suite owns the checked value
    //! that those writes consume, including its stored status spelling and borrowed failure.
    //! Small direct cases stay beside the constructor without a shared workflow fixture.

    use forgesync_core::coverage::{Failure, FailureKind};

    use crate::enumeration::RepositoryThreadScanStatus;
    use crate::enumeration::completion::ScanCompletion;
    use crate::error::StoreError;

    #[test]
    fn active_status_cannot_finalize_a_scan() {
        assert!(matches!(
            ScanCompletion::new(RepositoryThreadScanStatus::InProgress, None),
            Err(StoreError::InvalidRepositoryThreadScan)
        ));
    }

    #[test]
    fn complete_coverage_rejects_failure_evidence() {
        let failure = Failure {
            kind: FailureKind::Network,
            message: "request failed".to_owned(),
        };
        assert!(matches!(
            ScanCompletion::new(RepositoryThreadScanStatus::Complete, Some(&failure)),
            Err(StoreError::InvalidRepositoryThreadScan)
        ));
    }

    #[test]
    fn complete_coverage_has_no_diagnostic() {
        let completion = ScanCompletion::new(RepositoryThreadScanStatus::Complete, None)
            .expect("valid completion");
        assert_eq!(completion.status_name(), "complete");
        assert!(completion.failure().is_none());
    }

    #[test]
    fn interruption_can_finalize_without_a_provider_failure() {
        let completion = ScanCompletion::new(RepositoryThreadScanStatus::Incomplete, None)
            .expect("valid interruption");
        assert_eq!(completion.status_name(), "incomplete");
        assert!(completion.failure().is_none());
    }
}
