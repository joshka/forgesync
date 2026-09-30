//! # Apply one local member decision atomically
//!
//! `MemberDecisionWrite` binds a checked cluster, resolved current member, decision, reason, and
//! action time. `Archive` validates request size and identity, opens/fences the transaction, and
//! resolves membership before creating this prepared write.
//!
//! `apply` records the durable choice, updates current membership, clears a matching canonical
//! selection on exclusion, and appends its audit event. Inclusion restores active membership but
//! does not invent a canonical choice. Each phase shares the same member and action coordinates.
//!
//! The caller commits all phases together. Source discussions and generated representatives are
//! unchanged, and no GitHub write occurs. SQL projections stay explicit beside their policy.

use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;

use crate::clusters::decisions::insert_cluster_event;
use crate::error::StoreError;

/// Local member-state choice, independent of automatic generation membership.
#[derive(Clone, Copy)]
pub enum MemberDecision {
    /// Hide a current member and clear it if selected as canonical.
    Exclude,
    /// Restore a previously excluded member without selecting it as canonical.
    Include,
}

/// Prepared member mutation shared by durable choice, current state, and audit writes.
pub struct MemberDecisionWrite<'a> {
    /// Checked archive cluster row.
    pub cluster_id: i64,
    /// Current member resolved within the caller's fenced transaction.
    pub member_id: i64,
    /// Explicit inclusion policy applied to every related write.
    pub decision: MemberDecision,
    /// Validated reason, trimmed consistently at persistence boundaries.
    pub reason: &'a str,
    /// One action time shared by membership, canonical cleanup, and audit evidence.
    pub at: UtcTimestamp,
}

impl MemberDecisionWrite<'_> {
    /// Applies all member-decision effects without independently committing any phase.
    pub async fn apply(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        self.record(connection).await?;
        self.set_state(connection).await?;
        if matches!(self.decision, MemberDecision::Exclude) {
            self.clear_canonical(connection).await?;
        }
        self.audit(connection).await
    }

    /// Records the durable maintainer choice retained across regenerated membership.
    async fn record(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO cluster_member_decisions (cluster_id, thread_id, excluded, reason, updated_at_us) VALUES (?, ?, ?, ?, ?) ON CONFLICT (cluster_id, thread_id) DO UPDATE SET excluded = excluded.excluded, reason = excluded.reason, updated_at_us = excluded.updated_at_us",
        )
        .bind(self.cluster_id)
        .bind(self.member_id)
        .bind(self.decision.values().excluded)
        .bind(self.reason.trim())
        .bind(self.at.unix_microseconds())
        .execute(connection)
        .await?;
        Ok(())
    }

    /// Updates the current membership state using the same inclusion policy.
    async fn set_state(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        sqlx::query("UPDATE cluster_memberships SET state = ?, updated_at_us = ? WHERE cluster_id = ? AND thread_id = ?")
            .bind(self.decision.values().state)
            .bind(self.at.unix_microseconds())
            .bind(self.cluster_id)
            .bind(self.member_id)
            .execute(connection)
            .await?;
        Ok(())
    }

    /// Clears canonical selection only when this excluded member is the selected canonical row.
    async fn clear_canonical(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        sqlx::query("UPDATE clusters SET canonical_thread_id = NULL, updated_at_us = ? WHERE id = ? AND canonical_thread_id = ?")
                .bind(self.at.unix_microseconds())
                .bind(self.cluster_id)
                .bind(self.member_id)
                .execute(connection)
                .await?;
        Ok(())
    }

    /// Appends the corresponding maintainer event inside the same transaction.
    async fn audit(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        insert_cluster_event(
            connection,
            self.cluster_id,
            None,
            self.decision.values().event,
            Some(self.member_id),
            self.reason.trim(),
            self.at,
        )
        .await
    }
}

impl MemberDecision {
    /// Keeps the SQL exclusion flag, visible state, and audit label consistent for each choice.
    fn values(self) -> DecisionValues {
        match self {
            Self::Exclude => DecisionValues {
                excluded: 1,
                state: "excluded",
                event: "member_excluded",
            },
            Self::Include => DecisionValues {
                excluded: 0,
                state: "active",
                event: "member_included",
            },
        }
    }
}

/// Coupled archive encodings for one member inclusion choice.
struct DecisionValues {
    /// SQLite integer flag recorded in durable maintainer decisions.
    excluded: i64,
    /// Current membership state displayed by cluster queries.
    state: &'static str,
    /// Audit event label paired with that same choice.
    event: &'static str,
}
