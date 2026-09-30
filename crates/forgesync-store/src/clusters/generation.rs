//! # Commit one derived cluster generation
//!
//! `Archive::save_clusters_fenced` validates proposals before opening a write transaction, checks
//! the active writer fence, resolves source rows, and assigns durable cluster identities. It then
//! delegates generated membership application and commits once after all writes succeed.
//!
//! `generation_input` owns validation and row resolution; `generation_matching` owns overlap
//! assignment. `generation_apply` owns run identity, coverage policy, and accumulated write
//! results; `generation_rows` contains their SQL mutations. These helpers borrow the transaction
//! but never commit it. Candidate scoring remains in the engine, and local decisions are preserved
//! by the membership write policy. Failure before commit rolls back the entire generation.

use forgesync_core::timestamp::UtcTimestamp;

use crate::archive::Archive;
use crate::clusters::generation_apply::GenerationApplication;
use crate::clusters::generation_input::{prepare_clusters, repository_row_id, validate_generation};
use crate::clusters::generation_matching::{load_existing_clusters, match_cluster_identities};
use crate::clusters::{ClusterGenerationInput, ClusterGenerationResult};
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};

impl Archive {
    /// Saves a complete or partial generation under the active archive writer fence.
    ///
    /// Validates membership before opening the transaction, resolves archived identities inside
    /// it, preserves durable cluster IDs and local member decisions, then commits once. Missing
    /// members and unseen groups are retired only when vector coverage is complete. Source
    /// discussions, documents, and vectors are not rewritten.
    ///
    /// # Errors
    ///
    /// Invalid coverage/membership, unavailable writer access, lost fencing, unresolved source
    /// identities, and SQL failures prevent commit. Result count conversion follows commit under
    /// the existing outcome contract; those conversions retain their typed range errors.
    pub async fn save_clusters_fenced(
        &self,
        token: &ArchiveLeaseToken,
        input: &ClusterGenerationInput,
        at: UtcTimestamp,
    ) -> Result<ClusterGenerationResult, StoreError> {
        validate_generation(input)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let repository_row_id = repository_row_id(&mut transaction, &input.repository).await?;
        let prepared = prepare_clusters(&mut transaction, input, repository_row_id).await?;
        let existing = load_existing_clusters(&mut transaction, repository_row_id).await?;
        let matches = match_cluster_identities(&existing, &prepared);

        let mut application = GenerationApplication::start(
            &mut transaction,
            input,
            repository_row_id,
            prepared.len(),
            at,
        )
        .await?;
        application
            .apply(&mut transaction, &prepared, &matches)
            .await?;
        transaction.commit().await?;
        application.result()
    }
}
