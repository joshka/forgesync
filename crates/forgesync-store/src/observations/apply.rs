//! # Select and atomically apply a canonical parent observation
//!
//! `IncomingThread` prepares the incoming payload and source clock before opening a transaction.
//! Selection compares source revision, acquisition sequence, and completeness independently.
//! A complete identical older payload can hydrate evidence without replacing newer canonical data.
//!
//! `CanonicalSelection` carries the selected row and both high-water positions through writes.
//! Source content and complete-evidence clocks advance separately; neither implies child-family
//! completeness. The archive owns lease fencing and a single commit around all SQL effects.
//! `thread_rows` maps columns, while this module keeps the ordering and coverage policy visible.

use std::cmp::Ordering;

use forgesync_core::content::Discussion;
use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::ObservationSequence;
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observations::thread_rows::{
    StoredThreadObservation, ThreadPayloadUpdate, load_thread_observation, update_thread_payload,
};
use crate::observations::{
    ObservationDisposition, SourceClockColumns, ThreadObservationResult, normalize_source_clock,
    repository_row_id, source_clock_columns, sqlite_integer, to_sql_sequence, write_coverage,
};
use crate::ordering::compare_observation_order;

impl Archive {
    /// Applies one issue or pull-request snapshot using source clock and acquisition ordering.
    pub async fn apply_thread_observation(
        &self,
        observation: &Observation<Discussion>,
    ) -> Result<ThreadObservationResult, StoreError> {
        self.apply_thread_observation_inner(observation, None).await
    }

    /// Applies a thread snapshot only while the supplied archive lease remains current.
    pub async fn apply_thread_observation_fenced(
        &self,
        observation: &Observation<Discussion>,
        token: &ArchiveLeaseToken,
    ) -> Result<ThreadObservationResult, StoreError> {
        self.apply_thread_observation_inner(observation, Some(token))
            .await
    }

    /// Keeps canonical content, evidence clocks, and coverage in one optionally fenced transaction.
    async fn apply_thread_observation_inner(
        &self,
        observation: &Observation<Discussion>,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<ThreadObservationResult, StoreError> {
        if observation.family() != EvidenceFamily::Threads {
            return Err(StoreError::ObservationFamilyMismatch);
        }
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let incoming = IncomingThread::new(observation)?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let result = incoming.apply(&mut transaction).await?;
        transaction.commit().await?;
        Ok(result)
    }
}

/// Incoming domain observation plus the checked representations required for SQL application.
pub struct IncomingThread<'a> {
    /// Source value whose acquisition scope and completeness remain authoritative.
    pub observation: &'a Observation<Discussion>,
    /// Serialized canonical payload used for equality checks and row storage.
    pub payload_json: String,
    /// Normalized clock used by ordering policy.
    clock: SourceClock,
    /// Checked clock columns shared by payload and evidence writes.
    pub columns: SourceClockColumns,
    /// Checked SQLite representation of the public discussion number.
    pub number: i64,
}

impl<'a> IncomingThread<'a> {
    /// Serializes and checks the incoming representations before the transaction begins.
    fn new(observation: &'a Observation<Discussion>) -> Result<Self, StoreError> {
        let payload_json = serde_json::to_string(observation.payload())?;
        let clock = normalize_source_clock(observation.source_clock())?;
        let columns = source_clock_columns(&clock)?;
        let number = sqlite_integer(observation.payload().id.number().get())?;
        Ok(Self {
            observation,
            payload_json,
            clock,
            columns,
            number,
        })
    }

    /// Resolves local identity, selects canonical state, and applies only permitted changes.
    async fn apply(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<ThreadObservationResult, StoreError> {
        let discussion = self.observation.payload();
        let repository = repository_row_id(
            connection,
            discussion.id.repository().host().as_str(),
            discussion.id.repository().provider_id().as_str(),
        )
        .await?;
        let existing =
            load_thread_observation(connection, repository, discussion.id.provider_id().as_str())
                .await?;
        let selection = match existing {
            Some(existing) => self.select_existing(existing)?,
            None => self.select_new(connection, repository).await?,
        };
        match selection {
            ThreadSelection::Skipped(result) => Ok(result),
            ThreadSelection::Apply(selected) => selected.apply(connection, repository, self).await,
        }
    }

    /// Rejects older source revisions before applying acquisition-order and hydration policy.
    fn select_existing(
        &self,
        existing: StoredThreadObservation,
    ) -> Result<ThreadSelection, StoreError> {
        let sequence = self.observation.sequence();
        let order =
            compare_observation_order(&self.clock, sequence, &existing.source_clock, sequence)?;
        if order == Ordering::Less {
            return Ok(ThreadSelection::Skipped(existing.skipped()));
        }
        self.select_revision(existing, order)
    }

    /// Distinguishes conflicts, stale acquisitions, and complete identical-payload hydration.
    fn select_revision(
        &self,
        existing: StoredThreadObservation,
        source_order: Ordering,
    ) -> Result<ThreadSelection, StoreError> {
        let sequence = self.observation.sequence();
        let same_payload = self.payload_json == existing.payload_json;
        let order = compare_observation_order(
            &self.clock,
            sequence,
            &existing.source_clock,
            existing.sequence,
        )?;
        if source_order == Ordering::Equal && sequence == existing.sequence && !same_payload {
            return Err(StoreError::ConflictingObservation);
        }
        let hydrate = source_order == Ordering::Equal
            && sequence < existing.sequence
            && same_payload
            && matches!(
                self.observation.completeness(),
                CollectionCompleteness::Complete
            );
        if order == Ordering::Less && !hydrate {
            return Ok(ThreadSelection::Skipped(existing.skipped()));
        }
        let canonical_updated = order == Ordering::Greater;
        let high_water = if canonical_updated {
            sequence
        } else {
            existing.sequence
        };
        Ok(ThreadSelection::Apply(CanonicalSelection {
            id: existing.id,
            canonical_updated,
            high_water,
            evidence_clock: existing.evidence_clock,
            evidence_sequence: existing.evidence_sequence,
        }))
    }

    /// Inserts a new row whose complete evidence is then applied by the common selection path.
    async fn select_new(
        &self,
        connection: &mut SqliteConnection,
        repository: i64,
    ) -> Result<ThreadSelection, StoreError> {
        let id = self.insert(connection, repository).await?;
        let evidence_clock = if matches!(
            self.observation.completeness(),
            CollectionCompleteness::Complete
        ) {
            self.clock.clone()
        } else {
            SourceClock::Missing
        };
        Ok(ThreadSelection::Apply(CanonicalSelection {
            id,
            canonical_updated: true,
            high_water: self.observation.sequence(),
            evidence_clock,
            evidence_sequence: None,
        }))
    }
}

/// Canonical selection either preserves stored state or identifies the row allowed to change.
enum ThreadSelection {
    Skipped(ThreadObservationResult),
    Apply(CanonicalSelection),
}

/// Selected canonical row with independent source and complete-evidence high-water positions.
struct CanonicalSelection {
    id: i64,
    canonical_updated: bool,
    high_water: ObservationSequence,
    evidence_clock: SourceClock,
    evidence_sequence: Option<ObservationSequence>,
}

impl CanonicalSelection {
    /// Writes permitted payload/evidence changes and returns the resulting public positions.
    async fn apply(
        &self,
        connection: &mut SqliteConnection,
        repository: i64,
        incoming: &IncomingThread<'_>,
    ) -> Result<ThreadObservationResult, StoreError> {
        let evidence_sequence = self
            .accepts_evidence(incoming)?
            .then_some(incoming.observation.sequence());
        self.update(connection, repository, incoming, evidence_sequence)
            .await?;
        let disposition = if self.canonical_updated || evidence_sequence.is_some() {
            self.coverage(connection, incoming).await?;
            ObservationDisposition::Applied
        } else {
            ObservationDisposition::Replayed
        };
        let evidence_sequence = evidence_sequence.or(self.evidence_sequence);
        Ok(ThreadObservationResult {
            thread_row_id: self.id,
            disposition,
            high_water_sequence: self.high_water,
            evidence_sequence,
        })
    }

    /// Accepts only complete evidence newer than the current complete-evidence position.
    fn accepts_evidence(&self, incoming: &IncomingThread<'_>) -> Result<bool, StoreError> {
        if !matches!(
            incoming.observation.completeness(),
            CollectionCompleteness::Complete
        ) {
            return Ok(false);
        }
        match self.evidence_sequence {
            None => Ok(true),
            Some(sequence) => Ok(compare_observation_order(
                &incoming.clock,
                incoming.observation.sequence(),
                &self.evidence_clock,
                sequence,
            )? == Ordering::Greater),
        }
    }

    /// Writes canonical payload or evidence-only hydration, keeping those effects distinct.
    async fn update(
        &self,
        connection: &mut SqliteConnection,
        repository: i64,
        incoming: &IncomingThread<'_>,
        evidence_sequence: Option<ObservationSequence>,
    ) -> Result<(), StoreError> {
        if self.canonical_updated {
            let update = ThreadPayloadUpdate {
                discussion: incoming.observation.payload(),
                payload_json: &incoming.payload_json,
                source_clock: &incoming.columns,
                high_water_sequence: self.high_water,
                observed_at: incoming.observation.observed_at(),
                evidence_sequence,
            };
            update_thread_payload(connection, self.id, repository, incoming.number, update).await?;
        } else if evidence_sequence.is_some() {
            self.update_evidence(connection, incoming).await?;
        }
        Ok(())
    }

    /// Advances complete-evidence columns while retaining the selected canonical payload.
    async fn update_evidence(
        &self,
        connection: &mut SqliteConnection,
        incoming: &IncomingThread<'_>,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE threads SET evidence_clock_state = ?, evidence_clock_raw = ?, evidence_clock_us = ?, evidence_sequence = ? WHERE id = ?")
            .bind(incoming.columns.state).bind(&incoming.columns.raw).bind(incoming.columns.unix_microseconds)
            .bind(to_sql_sequence(incoming.observation.sequence())?).bind(self.id).execute(&mut *connection).await?;
        Ok(())
    }

    /// Derives parent coverage from the incoming completeness after a selected state change.
    async fn coverage(
        &self,
        connection: &mut SqliteConnection,
        incoming: &IncomingThread<'_>,
    ) -> Result<(), StoreError> {
        let observation = incoming.observation;
        let state = coverage_state(
            observation.completeness(),
            observation.observed_at(),
            observation.sequence(),
            1,
        );
        write_coverage(
            connection,
            self.id,
            EvidenceFamily::Threads,
            &incoming.columns,
            observation.observed_at(),
            observation.sequence(),
            &state,
        )
        .await
    }
}

impl StoredThreadObservation {
    /// Returns both stored positions unchanged when incoming ordering excludes application.
    fn skipped(&self) -> ThreadObservationResult {
        ThreadObservationResult {
            thread_row_id: self.id,
            disposition: ObservationDisposition::Skipped,
            high_water_sequence: self.sequence,
            evidence_sequence: self.evidence_sequence,
        }
    }
}
/// Derives coverage only from the declared completeness of the observation.
fn coverage_state(
    completeness: &CollectionCompleteness,
    observed_at: UtcTimestamp,
    sequence: ObservationSequence,
    item_count: u64,
) -> CoverageState {
    match completeness {
        CollectionCompleteness::Complete => CoverageState::Complete {
            observed_at,
            sequence,
            item_count,
        },
        CollectionCompleteness::Incomplete {
            reason,
            received_items,
        } => CoverageState::Incomplete {
            observed_at,
            sequence,
            reason: *reason,
            received_items: *received_items,
            failure: None,
        },
    }
}
