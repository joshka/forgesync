//! Repository registration by stable host/provider identity.
//!
//! Display fields are replaced on every call without timestamp ordering, so a rename keeps the
//! same local row. Registration creates no discussion evidence.

use forgesync_core::content::Repository;
use forgesync_core::timestamp::UtcTimestamp;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};

impl Archive {
    /// Registers the repository without a lease check, returning its local row key.
    pub async fn upsert_repository(&self, repository: &Repository) -> Result<i64, StoreError> {
        self.upsert_repository_inner(repository, None).await
    }

    /// Registers the repository while the supplied writer lease is active.
    pub async fn upsert_repository_fenced(
        &self,
        repository: &Repository,
        token: &ArchiveLeaseToken,
    ) -> Result<i64, StoreError> {
        self.upsert_repository_inner(repository, Some(token)).await
    }

    /// Upserts by host/provider identity, optionally checking the lease in the same transaction.
    ///
    /// Both public forms stay because the engine's sync module calls the fenced signature.
    async fn upsert_repository_inner(
        &self,
        repository: &Repository,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<i64, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let payload_json = serde_json::to_string(repository)?;
        let provider_data_json = serde_json::to_string(&repository.provider_data)?;
        let updated_at_us = repository.updated_at.map(UtcTimestamp::unix_microseconds);
        let host = repository.id.host().as_str();
        let provider_id = repository.id.provider_id().as_str();

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO repositories (host, provider_id, owner, name, full_name, default_branch, updated_at_us, provider_data_json, payload_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (host, provider_id) DO UPDATE SET owner = excluded.owner, name = excluded.name, full_name = excluded.full_name, default_branch = excluded.default_branch, updated_at_us = excluded.updated_at_us, provider_data_json = excluded.provider_data_json, payload_json = excluded.payload_json RETURNING id",
        )
        .bind(host)
        .bind(provider_id)
        .bind(&repository.owner)
        .bind(&repository.name)
        .bind(&repository.full_name)
        .bind(&repository.default_branch)
        .bind(updated_at_us)
        .bind(provider_data_json)
        .bind(payload_json)
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(id)
    }
}
