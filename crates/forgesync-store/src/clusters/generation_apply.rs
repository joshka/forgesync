//! # Apply prepared membership within one generation transaction
//!
//! `GenerationApplication` retains the run, repository, timestamp, and coverage policy shared by
//! generated cluster writes. It accumulates seen cluster identities and persisted member count,
//! then finalizes the run. The archive entry point owns the transaction and its single commit.
//!
//! Each prepared group reuses its assigned durable identity or receives a membership-derived key.
//! Member movement preserves local exclusion decisions. Missing members and unseen groups are
//! retired only for complete vector coverage; partial generations cannot erase unseen evidence.
//! SQL mutations live in `generation_rows`, while this module makes their ordering explicit.
//!
//! Results are projected after the caller commits, preserving the existing count-conversion error
//! contract. Errors during application prevent commit and roll back all generation writes.

use std::collections::HashMap;

use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;

use crate::clusters::decisions::insert_cluster_event;
use crate::clusters::generation_input::PreparedCluster;
use crate::clusters::generation_rows::{
    finish_cluster_run, insert_cluster_run, mark_missing_members_removed, remove_other_memberships,
    retire_unseen_clusters, upsert_generated_cluster, upsert_generated_member,
};
use crate::clusters::{ClusterGenerationInput, ClusterGenerationResult};
use crate::error::StoreError;

/// Generation-wide write facts and accumulated membership outcomes.
pub struct GenerationApplication<'a> {
    /// Coverage and service identity of the proposed generation, fixed throughout application.
    input: &'a ClusterGenerationInput,
    /// Resolved repository row receiving this generation.
    repository_id: i64,
    /// Durable run row shared by generated memberships and events.
    run_id: i64,
    /// One timestamp shared by every mutation in the generation.
    at: UtcTimestamp,
    /// Generated cluster rows retained for complete-coverage retirement exclusion.
    seen: Vec<i64>,
    /// Number of generated memberships written successfully so far.
    member_count: u64,
    /// Groups retired by complete replacement; remains zero for partial coverage.
    retired_count: usize,
}

impl<'a> GenerationApplication<'a> {
    /// Records the run before member writes and binds its identity to one proposed generation.
    pub async fn start(
        connection: &mut SqliteConnection,
        input: &'a ClusterGenerationInput,
        repository_id: i64,
        cluster_count: usize,
        at: UtcTimestamp,
    ) -> Result<Self, StoreError> {
        let run_id =
            insert_cluster_run(connection, input, repository_id, cluster_count, at).await?;
        Ok(Self {
            input,
            repository_id,
            run_id,
            at,
            seen: Vec::with_capacity(cluster_count),
            member_count: 0,
            retired_count: 0,
        })
    }

    /// Writes prepared groups in order, retires unseen complete-scope groups, and finalizes the
    /// run.
    pub async fn apply(
        &mut self,
        connection: &mut SqliteConnection,
        prepared: &[PreparedCluster],
        matches: &HashMap<usize, i64>,
    ) -> Result<(), StoreError> {
        for (index, cluster) in prepared.iter().enumerate() {
            self.apply_cluster(connection, cluster, matches.get(&index).copied())
                .await?;
        }
        if self.input.complete_coverage {
            self.retired_count =
                retire_unseen_clusters(connection, self.repository_id, &self.seen, self.at).await?;
        }
        finish_cluster_run(
            connection,
            self.run_id,
            self.input,
            prepared.len(),
            self.member_count,
            self.at,
        )
        .await
    }

    /// Applies one identity, membership replacement policy, generated members, and audit event.
    async fn apply_cluster(
        &mut self,
        connection: &mut SqliteConnection,
        cluster: &PreparedCluster,
        matched_id: Option<i64>,
    ) -> Result<(), StoreError> {
        let cluster_id = upsert_generated_cluster(
            connection,
            self.repository_id,
            self.run_id,
            matched_id,
            cluster,
            self.at,
        )
        .await?;
        self.seen.push(cluster_id);
        let member_ids = cluster
            .members
            .iter()
            .map(|(thread_id, _)| *thread_id)
            .collect::<Vec<_>>();
        remove_other_memberships(connection, cluster_id, &member_ids, self.at).await?;
        if self.input.complete_coverage {
            mark_missing_members_removed(connection, cluster_id, &member_ids, self.at).await?;
        }
        self.apply_members(connection, cluster_id, cluster).await?;
        insert_cluster_event(
            connection,
            cluster_id,
            Some(self.run_id),
            "generated",
            None,
            "",
            self.at,
        )
        .await?;
        Ok(())
    }

    /// Counts each successfully written member while retaining the store's local decision policy.
    async fn apply_members(
        &mut self,
        connection: &mut SqliteConnection,
        cluster_id: i64,
        cluster: &PreparedCluster,
    ) -> Result<(), StoreError> {
        for (thread_id, score) in &cluster.members {
            upsert_generated_member(
                connection,
                cluster_id,
                *thread_id,
                *score,
                self.run_id,
                self.at,
            )
            .await?;
            self.member_count = self.member_count.saturating_add(1);
        }
        Ok(())
    }

    /// Projects committed outcomes without changing the existing integer-conversion
    /// classifications.
    pub fn result(self) -> Result<ClusterGenerationResult, StoreError> {
        Ok(ClusterGenerationResult {
            run_id: u64::try_from(self.run_id).map_err(|_| StoreError::InvalidStoredCount)?,
            cluster_count: u64::try_from(self.seen.len())
                .map_err(|_| StoreError::IntegerOutOfRange)?,
            member_count: self.member_count,
            retired_count: u64::try_from(self.retired_count)
                .map_err(|_| StoreError::IntegerOutOfRange)?,
            complete_coverage: self.input.complete_coverage,
        })
    }
}
