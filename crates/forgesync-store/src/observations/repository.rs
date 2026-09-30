//! # Register current repository descriptions
//!
//! A normalized thread belongs to a repository and provider host. These `Archive` helpers find or
//! create the local repository row needed before a thread observation can be applied.
//!
//! The identity conversion is kept next to the write boundary so engine code deals in domain
//! repository identifiers rather than SQL row IDs. Repository scope is also used by enumeration,
//! search, and reporting, so incorrect resolution would affect more than a single thread write.
//!
//! [`Archive::upsert_repository`] and [`Archive::upsert_repository_fenced`] both match by stable
//! host/provider identity. Owner, name, full name, default branch, update timestamp, and extension
//! payload are replaceable current descriptions, not the row's identity. A rename therefore keeps
//! the same local repository key. The returned `i64` is that SQLite key, not the provider ID.
//!
//! Unlike discussion observation application, this registration write has no acquisition sequence
//! or source-clock ordering guard. Every successful call replaces the descriptive payload supplied
//! by the caller, even when its optional update time is older or absent. Workflows own choosing
//! the repository description to register; this module does not merge historical descriptions.
//!
//! The fenced operation validates the token in the same transaction as the upsert. The unfenced
//! operation deliberately bypasses that workflow lease check and is used for explicit local setup.
//! Both require a writable archive, perform only SQLite work, and commit before returning the key.
//! Registration neither fetches discussions nor creates complete evidence for any child family.

use forgesync_core::content::Repository;
use forgesync_core::timestamp::UtcTimestamp;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};

impl Archive {
    /// Registers the supplied current description without requiring a workflow lease.
    ///
    /// Matches stable host/provider identity and returns the local SQLite row key. Existing display
    /// fields and payload are replaced without comparing source timestamps; callers own selecting
    /// current input. Use [`Self::upsert_repository_fenced`] for coordinated workflow writes.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ReadOnlyArchive`] without a writer and propagates serialization or
    /// database errors. Failure before commit leaves this registration update unapplied.
    pub async fn upsert_repository(&self, repository: &Repository) -> Result<i64, StoreError> {
        self.upsert_repository_inner(repository, None).await
    }

    /// Registers the description while this owner holds an active archive lease.
    ///
    /// The lease check and same host/provider upsert share one transaction. Lease authority does
    /// not establish description freshness: fields are replaced without timestamp ordering, using
    /// the same contract as [`Self::upsert_repository`]. Returns the stable local row key.
    ///
    /// # Errors
    ///
    /// Returns read-only, lease/clock, serialization, or database errors. A stale or expired token
    /// rejects the registration before its payload is written.
    pub async fn upsert_repository_fenced(
        &self,
        repository: &Repository,
        token: &ArchiveLeaseToken,
    ) -> Result<i64, StoreError> {
        self.upsert_repository_inner(repository, Some(token)).await
    }

    /// Updates repository identity and current name under optional lease fencing.
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
