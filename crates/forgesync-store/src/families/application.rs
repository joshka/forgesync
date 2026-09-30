//! # Transaction-local promotion of a reserved child collection
//!
//! `FamilyApplication` joins a terminal observation with its SQL identity, source clock, and
//! staged pages. Preparation verifies the current reservation and distinguishes an already
//! completed replay from work that still needs application. No provider I/O occurs here.
//!
//! Complete application validates pages, replaces membership, writes coverage/head context, and
//! removes obsolete staging state. Incomplete application verifies received counts and records
//! coverage without replacing canonical members. All methods borrow the same SQLite connection
//! from the caller's transaction; `finish` alone commits it after the entire application succeeds.

use forgesync_core::coverage::CoverageState;
use forgesync_core::observation::{CollectionCompleteness, IncompleteReason};
use sqlx::{Row, SqliteConnection};

use crate::error::StoreError;
use crate::families::staging::{
    count_staged_items, load_staged_pages, merge_staged_items, validate_page_set,
};
use crate::families::{ChildFamilyObservation, StagedPage};
use crate::observation_sql::{
    SourceClockColumns, evidence_family_name, source_clock_columns, source_clock_from_columns,
    thread_row_id, write_coverage,
};
use crate::observations::{FamilyObservationResult, ObservationDisposition};

/// Preparation either returns a terminal replay/skip or a pending collection to apply.
pub enum FamilyFinalization<'a> {
    /// No membership write is needed for a stale reservation or complete replay.
    Finished(FamilyObservationResult),
    /// The reserved collection still needs complete or incomplete application.
    Pending(FamilyApplication<'a>),
}

/// Validated reservation scope and staged data, usable only inside its caller's transaction.
pub struct FamilyApplication<'a> {
    observation: ChildFamilyObservation<'a>,
    thread: i64,
    sequence: i64,
    source_clock: SourceClockColumns,
    pages: Vec<StagedPage>,
    staged_count: u64,
}

impl<'a> FamilyApplication<'a> {
    /// Checks reservation ownership and loads the generation before any membership replacement.
    pub async fn prepare(
        connection: &mut SqliteConnection,
        observation: ChildFamilyObservation<'a>,
        sequence: i64,
    ) -> Result<FamilyFinalization<'a>, StoreError> {
        let thread = thread_row_id(connection, observation.thread).await?;
        let family = evidence_family_name(observation.family);
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT sequence FROM thread_family_reservations WHERE thread_id = ? AND family = ?",
        )
        .bind(thread)
        .bind(family)
        .fetch_optional(&mut *connection)
        .await?;
        if current != Some(sequence) {
            return Ok(FamilyFinalization::Finished(FamilyObservationResult {
                disposition: ObservationDisposition::Skipped,
                item_count: 0,
            }));
        }
        let generation = Generation::load(connection, thread, family, sequence).await?;
        if generation.status == "complete" {
            return Ok(FamilyFinalization::Finished(generation.replay()?));
        }
        let source_clock = source_clock_columns(&generation.clock()?)?;
        let pages = load_staged_pages(connection, thread, family, sequence).await?;
        let staged_count = count_staged_items(&pages)?;
        Ok(FamilyFinalization::Pending(Self {
            observation,
            thread,
            sequence,
            source_clock,
            pages,
            staged_count,
        }))
    }

    /// Selects the membership-changing or coverage-only path from declared completeness.
    pub async fn apply(
        self,
        connection: &mut SqliteConnection,
    ) -> Result<FamilyObservationResult, StoreError> {
        match self.observation.completeness {
            CollectionCompleteness::Complete => self.complete(connection).await,
            CollectionCompleteness::Incomplete {
                reason,
                received_items,
            } => self.incomplete(connection, *reason, *received_items).await,
        }
    }

    /// Promotes validated complete membership and its coverage before clearing obsolete attempts.
    async fn complete(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<FamilyObservationResult, StoreError> {
        let expected = self
            .observation
            .expected_pages
            .ok_or(StoreError::MissingExpectedPageCount)?;
        validate_page_set(&self.pages, expected)?;
        let items = merge_staged_items(&self.pages)?;
        self.clear_members(connection).await?;
        for item in items.values() {
            self.insert_member(connection, item).await?;
        }
        let item_count = u64::try_from(items.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
        let state = CoverageState::Complete {
            observed_at: self.observation.observed_at,
            sequence: self.observation.sequence,
            item_count,
        };
        self.coverage(connection, &state).await?;
        self.head_context(connection).await?;
        self.finish_generation(connection, "complete", self.staged_count, item_count)
            .await?;
        self.clear_staging(connection).await?;
        Ok(FamilyObservationResult {
            disposition: ObservationDisposition::Applied,
            item_count,
        })
    }

    /// Records partial coverage only when its received count agrees with durable staged pages.
    ///
    /// Coverage uses this reservation's observation time and sequence, never caller-assembled
    /// coordinates. The supplied reason/count are terminal collection facts. Canonical membership
    /// and complete head context remain unchanged; the caller still owns transaction commit.
    async fn incomplete(
        &self,
        connection: &mut SqliteConnection,
        reason: IncompleteReason,
        received: u64,
    ) -> Result<FamilyObservationResult, StoreError> {
        if received != self.staged_count {
            return Err(StoreError::InvalidCollectionCompleteness);
        }
        let state = CoverageState::Incomplete {
            observed_at: self.observation.observed_at,
            sequence: self.observation.sequence,
            reason,
            received_items: received,
            failure: None,
        };
        self.coverage(connection, &state).await?;
        self.finish_generation(connection, "incomplete", received, self.staged_count)
            .await?;
        Ok(FamilyObservationResult {
            disposition: ObservationDisposition::Applied,
            item_count: self.staged_count,
        })
    }

    /// Removes canonical members before inserting the validated replacement in the same
    /// transaction.
    async fn clear_members(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM thread_family_membership WHERE thread_id = ? AND family = ?")
            .bind(self.thread)
            .bind(self.family())
            .execute(&mut *connection)
            .await?;
        Ok(())
    }

    /// Inserts one normalized member with the observation's reserved sequence.
    async fn insert_member(
        &self,
        connection: &mut SqliteConnection,
        item: &crate::observations::StagedItem<serde_json::Value>,
    ) -> Result<(), StoreError> {
        let payload = serde_json::to_string(&item.payload)?;
        sqlx::query("INSERT INTO thread_family_membership (thread_id, family, provider_id, payload_json, sequence) VALUES (?, ?, ?, ?, ?)")
            .bind(self.thread).bind(self.family()).bind(item.id.as_str()).bind(payload).bind(self.sequence)
            .execute(&mut *connection).await?;
        Ok(())
    }

    /// Writes coverage using the source clock retained on the reserved generation.
    async fn coverage(
        &self,
        connection: &mut SqliteConnection,
        state: &CoverageState,
    ) -> Result<(), StoreError> {
        write_coverage(
            connection,
            self.thread,
            self.observation.family,
            &self.source_clock,
            self.observation.observed_at,
            self.observation.sequence,
            state,
        )
        .await
    }

    /// Associates complete review evidence with its validated pull-request head.
    async fn head_context(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        let Some(head) = self.observation.head_sha else {
            return Ok(());
        };
        sqlx::query("INSERT INTO thread_family_head_contexts (thread_id, family, head_sha, sequence) VALUES (?, ?, ?, ?) ON CONFLICT (thread_id, family) DO UPDATE SET head_sha = excluded.head_sha, sequence = excluded.sequence")
            .bind(self.thread).bind(self.family()).bind(head.as_str()).bind(self.sequence)
            .execute(&mut *connection).await?;
        Ok(())
    }

    /// Marks the generation terminal with separate received and unique-member counts.
    async fn finish_generation(
        &self,
        connection: &mut SqliteConnection,
        status: &str,
        received: u64,
        items: u64,
    ) -> Result<(), StoreError> {
        let received = i64::try_from(received).map_err(|_| StoreError::IntegerOutOfRange)?;
        let items = i64::try_from(items).map_err(|_| StoreError::IntegerOutOfRange)?;
        sqlx::query("UPDATE observation_generations SET status = ?, received_items = ?, item_count = ? WHERE thread_id = ? AND family = ? AND sequence = ?")
            .bind(status).bind(received).bind(items).bind(self.thread).bind(self.family()).bind(self.sequence)
            .execute(&mut *connection).await?;
        Ok(())
    }

    /// Deletes provisional pages and older generations only after complete canonical application.
    async fn clear_staging(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM observation_staging_pages WHERE thread_id = ? AND family = ?")
            .bind(self.thread)
            .bind(self.family())
            .execute(&mut *connection)
            .await?;
        sqlx::query("DELETE FROM observation_generations WHERE thread_id = ? AND family = ? AND sequence <> ?")
            .bind(self.thread).bind(self.family()).bind(self.sequence).execute(&mut *connection).await?;
        Ok(())
    }

    /// Returns the stable SQL label shared by this observation's tables.
    fn family(&self) -> &'static str {
        evidence_family_name(self.observation.family)
    }
}

/// Stored generation fields needed to recognize a replay or recover its source clock.
struct Generation {
    status: String,
    items: i64,
    source_state: String,
    source_raw: String,
    source_us: Option<i64>,
}

impl Generation {
    /// Loads the reserved generation, failing if its reservation has no matching record.
    async fn load(
        connection: &mut SqliteConnection,
        thread: i64,
        family: &str,
        sequence: i64,
    ) -> Result<Self, StoreError> {
        let row = sqlx::query("SELECT source_clock_state, source_clock_raw, source_clock_us, status, item_count FROM observation_generations WHERE thread_id = ? AND family = ? AND sequence = ?")
            .bind(thread).bind(family).bind(sequence).fetch_optional(&mut *connection).await?
            .ok_or(StoreError::ObservationGenerationMissing)?;
        Ok(Self {
            status: row.try_get("status")?,
            items: row.try_get("item_count")?,
            source_state: row.try_get("source_clock_state")?,
            source_raw: row.try_get("source_clock_raw")?,
            source_us: row.try_get("source_clock_us")?,
        })
    }

    /// Returns the prior complete result without reapplying its membership.
    fn replay(&self) -> Result<FamilyObservationResult, StoreError> {
        let item_count =
            u64::try_from(self.items).map_err(|_| StoreError::InvalidStoredSequence)?;
        Ok(FamilyObservationResult {
            disposition: ObservationDisposition::Replayed,
            item_count,
        })
    }

    /// Reconstructs the source clock whose ordering established this reservation.
    fn clock(&self) -> Result<forgesync_core::observation::SourceClock, StoreError> {
        source_clock_from_columns(&self.source_state, &self.source_raw, self.source_us)
    }
}
