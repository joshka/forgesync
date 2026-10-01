//! Failure ledger writes, scoped to the unit of work that failed.
//!
//! Retry/resolution of earlier failures always excludes entries created by the current run, so new
//! failures from that same attempt are preserved. Retry marking is not idempotent.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::timestamp::UtcTimestamp;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::runs::{ChildFamilyFailureScope, RunFailureInput, RunFailureScope, to_sql_id};
use crate::sql::{repository_row_id, to_sql_integer};

impl Archive {
    /// Increments retry counts of earlier unresolved selector failures (rows without a
    /// repository) in exactly this scope, returning the affected count.
    pub async fn mark_scope_failures_retried(
        &self,
        token: &ArchiveLeaseToken,
        scope: &RunFailureScope<'_>,
    ) -> Result<u64, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let result = sqlx::query(
            "UPDATE failures SET retry_count = retry_count + 1, retry_run_id = ? WHERE repository_id IS NULL AND target_key = ? AND family = ? AND scope_key = ? AND resolved_at_us IS NULL AND run_id <> ?",
        )
        .bind(to_sql_id(scope.run_id)?)
        .bind(scope.target)
        .bind(scope.family.as_str())
        .bind(scope.scope_key)
        .bind(to_sql_id(scope.run_id)?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result.rows_affected())
    }

    /// Resolves earlier selector failures in exactly this scope, keeping any retry provenance.
    ///
    /// The caller must have established that the matching work succeeded.
    pub async fn resolve_scope_failures(
        &self,
        token: &ArchiveLeaseToken,
        scope: &RunFailureScope<'_>,
        resolved_at: UtcTimestamp,
    ) -> Result<u64, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let result = sqlx::query(
            "UPDATE failures SET resolved_at_us = ?, retry_run_id = COALESCE(retry_run_id, ?) WHERE repository_id IS NULL AND target_key = ? AND family = ? AND scope_key = ? AND resolved_at_us IS NULL AND run_id <> ?",
        )
        .bind(resolved_at.unix_microseconds())
        .bind(to_sql_id(scope.run_id)?)
        .bind(scope.target)
        .bind(scope.family.as_str())
        .bind(scope.scope_key)
        .bind(to_sql_id(scope.run_id)?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result.rows_affected())
    }

    /// Records a workflow failure with optional repository and thread scope.
    ///
    /// A supplied thread requires the same repository ([`StoreError::InvalidRunData`] otherwise).
    pub async fn record_run_failure(
        &self,
        token: &ArchiveLeaseToken,
        failure: RunFailureInput<'_>,
    ) -> Result<(), StoreError> {
        if let Some(thread) = failure.thread {
            let Some(repository) = failure.repository else {
                return Err(StoreError::InvalidRunData);
            };
            if repository != thread.repository() {
                return Err(StoreError::InvalidRunData);
            }
        }
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let failure_json = serde_json::to_string(failure.failure)?;
        let repository_id = match failure.repository {
            Some(repository) => Some(repository_row_id(&mut transaction, repository).await?),
            None => None,
        };
        let thread_number = failure
            .thread
            .map(|thread| to_sql_integer(thread.number().get()))
            .transpose()?;
        let thread_provider_id = failure.thread.map(|thread| thread.provider_id().as_str());
        sqlx::query(
            "INSERT INTO failures (run_id, repository_id, family, target_key, scope_key, failure_json, created_at_us, thread_provider_id, thread_number) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(to_sql_id(failure.run_id)?)
        .bind(repository_id)
        .bind(failure.family.map(EvidenceFamily::as_str))
        .bind(failure.target)
        .bind(failure.scope_key)
        .bind(failure_json)
        .bind(failure.created_at.unix_microseconds())
        .bind(thread_provider_id)
        .bind(thread_number)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Increments retry counts of earlier unresolved failures for exactly this child-family scope,
    /// returning the affected count.
    pub async fn mark_child_family_failures_retried(
        &self,
        token: &ArchiveLeaseToken,
        scope: &ChildFamilyFailureScope<'_>,
    ) -> Result<u64, StoreError> {
        if scope.repository != scope.thread.repository() {
            return Err(StoreError::InvalidRunData);
        }
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let repository_id = repository_row_id(&mut transaction, scope.repository).await?;
        let result = sqlx::query(
            "UPDATE failures SET retry_count = retry_count + 1, retry_run_id = ? WHERE repository_id = ? AND thread_provider_id = ? AND thread_number = ? AND family = ? AND scope_key = ? AND resolved_at_us IS NULL AND run_id <> ?",
        )
        .bind(to_sql_id(scope.run_id)?)
        .bind(repository_id)
        .bind(scope.thread.provider_id().as_str())
        .bind(to_sql_integer(scope.thread.number().get())?)
        .bind(scope.family.as_str())
        .bind(scope.scope_key)
        .bind(to_sql_id(scope.run_id)?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result.rows_affected())
    }

    /// Resolves earlier failures for exactly this child-family scope, keeping retry provenance.
    ///
    /// The caller must have established that the matching collection completed.
    pub async fn resolve_child_family_failures(
        &self,
        token: &ArchiveLeaseToken,
        scope: &ChildFamilyFailureScope<'_>,
        resolved_at: UtcTimestamp,
    ) -> Result<u64, StoreError> {
        if scope.repository != scope.thread.repository() {
            return Err(StoreError::InvalidRunData);
        }
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let repository_id = repository_row_id(&mut transaction, scope.repository).await?;
        let result = sqlx::query(
            "UPDATE failures SET resolved_at_us = ?, retry_run_id = COALESCE(retry_run_id, ?) WHERE repository_id = ? AND thread_provider_id = ? AND thread_number = ? AND family = ? AND scope_key = ? AND resolved_at_us IS NULL AND run_id <> ?",
        )
        .bind(resolved_at.unix_microseconds())
        .bind(to_sql_id(scope.run_id)?)
        .bind(repository_id)
        .bind(scope.thread.provider_id().as_str())
        .bind(to_sql_integer(scope.thread.number().get())?)
        .bind(scope.family.as_str())
        .bind(scope.scope_key)
        .bind(to_sql_id(scope.run_id)?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result.rows_affected())
    }
}
