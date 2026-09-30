//! # Record failures at the unit of work that failed
//!
//! These `Archive` methods persist provider or application failures with enough scope to identify
//! the affected run, job, thread, and family. A failed child collection should not erase
//! successful work elsewhere in the same run.
//!
//! The engine uses these records for partial reports and retries. Storing failure scope explicitly
//! avoids inferring it from missing rows, which cannot distinguish an unrequested resource from
//! one that was attempted and failed.
//!
//! [`Archive::record_run_failure`] accepts failures both before and after repository resolution.
//! Thread scope requires a matching supplied repository, but recording the ledger entry does not
//! acquire evidence, update coverage, or complete a job. The operation retains original failure
//! JSON so later retries and history can explain the failed attempt.
//!
//! Selector-scoped retry/resolution targets unresolved rows without a resolved repository.
//! Child-family retry/resolution uses registered repository and exact thread identity, family,
//! and scope key. Both exclude entries created by the current run, preserving new failures from
//! that same attempt. A successful mutation can affect zero prior rows.
//!
//! Retry marking increments counts and records the current retry run; repeated marking is not
//! idempotent. Resolution timestamps matching unresolved rows without deleting history or proving
//! provider completion. The engine must invoke resolution only after its matching work succeeds.
//! All mutations validate the active archive lease within their transaction, but that fence alone
//! does not prove that the workflow selected the correct failure scope.

use forgesync_core::timestamp::UtcTimestamp;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observation_sql::{evidence_family_name, repository_row_id};
use crate::runs::{
    ChildFamilyFailureScope, RunFailureInput, RunFailureScope, to_sql_id, to_sql_id_u64,
};

impl Archive {
    /// Increments retry counts for prior unresolved selector failures in the exact supplied scope.
    ///
    /// Matches target, family, and scope key only on rows without a repository, excluding this
    /// run's own failures. Returns the affected count, including zero. Repeating the call
    /// increments again; the engine owns calling it once for the selected retry attempt.
    ///
    /// Read-only, lease, run-ID conversion, and database errors reject the transaction.
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
        .bind(evidence_family_name(scope.family))
        .bind(scope.scope_key)
        .bind(to_sql_id(scope.run_id)?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result.rows_affected())
    }

    /// Resolves prior selector failures after the caller establishes matching work completion.
    ///
    /// Uses the same exact scope and current-run exclusion as retry marking. Records `resolved_at`
    /// and fills the retry run only when absent; existing retry provenance is retained. Returns the
    /// affected count without deleting history. Repeating resolution affects zero already resolved
    /// rows. This method does not check coverage or independently prove successful acquisition.
    ///
    /// Read-only, lease, run-ID conversion, and database errors reject the transaction.
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
        .bind(evidence_family_name(scope.family))
        .bind(scope.scope_key)
        .bind(to_sql_id(scope.run_id)?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result.rows_affected())
    }

    /// Records a workflow failure with optional resolved repository and thread scope.
    ///
    /// A supplied thread requires a repository with the same stable identity; otherwise returns
    /// [`StoreError::InvalidRunData`] before writing. A supplied repository must be registered.
    /// The failure payload and scope are recorded without validating acquisition completeness or
    /// updating coverage. Each call inserts a new historical entry rather than deduplicating it.
    ///
    /// Read-only, lease, identity conversion, serialization, and database errors are propagated.
    /// A successful entry does not imply that the enclosing run or job has been finished.
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
            Some(repository) => Some(
                repository_row_id(
                    &mut transaction,
                    repository.host().as_str(),
                    repository.provider_id().as_str(),
                )
                .await?,
            ),
            None => None,
        };
        let thread_number = failure
            .thread
            .map(|thread| to_sql_id_u64(thread.number().get()))
            .transpose()?;
        let thread_provider_id = failure.thread.map(|thread| thread.provider_id().as_str());
        sqlx::query(
            "INSERT INTO failures (run_id, repository_id, family, target_key, scope_key, failure_json, created_at_us, thread_provider_id, thread_number) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(to_sql_id(failure.run_id)?)
        .bind(repository_id)
        .bind(failure.family.map(evidence_family_name))
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

    /// Increments retry counts for prior unresolved failures matching this exact child-family
    /// scope.
    ///
    /// Requires matching repository/thread identity and a registered repository. Matches both
    /// thread provider ID and local number, family, and scope key; excludes this run's
    /// failures. Returns affected rows, including zero. Repeated calls increment again rather
    /// than being idempotent.
    ///
    /// Scope mismatch, read-only, lease, identity conversion, and database failures reject writing.
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
        let repository_id = repository_row_id(
            &mut transaction,
            scope.repository.host().as_str(),
            scope.repository.provider_id().as_str(),
        )
        .await?;
        let result = sqlx::query(
            "UPDATE failures SET retry_count = retry_count + 1, retry_run_id = ? WHERE repository_id = ? AND thread_provider_id = ? AND thread_number = ? AND family = ? AND scope_key = ? AND resolved_at_us IS NULL AND run_id <> ?",
        )
        .bind(to_sql_id(scope.run_id)?)
        .bind(repository_id)
        .bind(scope.thread.provider_id().as_str())
        .bind(to_sql_id_u64(scope.thread.number().get())?)
        .bind(evidence_family_name(scope.family))
        .bind(scope.scope_key)
        .bind(to_sql_id(scope.run_id)?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result.rows_affected())
    }

    /// Resolves prior child-family failures after the caller proves matching collection completion.
    ///
    /// Uses the exact identity/family/scope and current-run exclusion of retry marking. Retains
    /// existing retry provenance and sets the supplied resolution time without deleting entries.
    /// Returns zero for an absent or already resolved prior scope. This method checks identity and
    /// lease authority, not stored collection completeness; the engine owns that prerequisite.
    ///
    /// Scope mismatch, read-only, lease, identity conversion, and database failures reject writing.
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
        let repository_id = repository_row_id(
            &mut transaction,
            scope.repository.host().as_str(),
            scope.repository.provider_id().as_str(),
        )
        .await?;
        let result = sqlx::query(
            "UPDATE failures SET resolved_at_us = ?, retry_run_id = COALESCE(retry_run_id, ?) WHERE repository_id = ? AND thread_provider_id = ? AND thread_number = ? AND family = ? AND scope_key = ? AND resolved_at_us IS NULL AND run_id <> ?",
        )
        .bind(resolved_at.unix_microseconds())
        .bind(to_sql_id(scope.run_id)?)
        .bind(repository_id)
        .bind(scope.thread.provider_id().as_str())
        .bind(to_sql_id_u64(scope.thread.number().get())?)
        .bind(evidence_family_name(scope.family))
        .bind(scope.scope_key)
        .bind(to_sql_id(scope.run_id)?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result.rows_affected())
    }
}
