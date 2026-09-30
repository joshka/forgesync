//! # Reserve ordering before child-family I/O
//!
//! Reservation allocates an observation sequence before the engine asks GitHub for pages. That
//! sequence gives the later complete or incomplete result a stable acquisition position, even when
//! provider calls finish out of order.
//!
//! The reservation is a durable precursor, not a declaration of complete coverage. `staging` adds
//! pages and `finish` decides which result can change canonical membership.
//!
//! The archive methods validate the request and own the transaction. `ReservedGeneration` keeps
//! the proposed identity and clocks together for comparison and persistence. Rejected proposals
//! still consume acquisition order, but never replace a newer reservation or create a generation.

use forgesync_core::identity::ObservationSequence;
use forgesync_core::observation::SourceClock;
use forgesync_core::timestamp::UtcTimestamp;
use sqlx::{Row, SqliteConnection};

use crate::archive::Archive;
use crate::error::StoreError;
use crate::families::ChildFamilyRequest;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observation_sql::{
    SourceClockColumns, checked_sequence, evidence_family_name, is_child_family,
    normalize_source_clock, source_clock_columns, source_clock_from_columns, thread_row_id,
    to_sql_sequence,
};
use crate::observations::FamilyReservation;
use crate::ordering::compare_observation_order;

impl Archive {
    /// Allocates acquisition order before fetching one discussion's child evidence.
    ///
    /// Call this before provider I/O, then stage pages and finalize with the returned sequence.
    /// `request.family` must identify comments, metadata, reviews, or review threads; parent thread
    /// scans use their own observation path. `request.request_scope` must contain a nonempty
    /// description of the selected provider request, while `request.source_clock` describes
    /// source freshness rather than the local start time.
    ///
    /// A result with `reserved == false` still consumes a sequence but leaves the newer reservation
    /// intact. Do not fetch or stage that rejected generation. A successful reservation does not
    /// replace canonical membership or declare complete coverage.
    ///
    /// # Errors
    ///
    /// Returns an error for a read-only archive, unknown thread, unsupported family, invalid scope
    /// or clock, or failed database transaction. Writes commit together before returning; no
    /// transaction is held while the caller performs network I/O.
    pub async fn reserve_child_family_observation(
        &self,
        request: ChildFamilyRequest<'_>,
    ) -> Result<FamilyReservation, StoreError> {
        self.reserve_child_family_observation_inner(request, None)
            .await
    }

    /// Reserves child evidence under an archive writer fence.
    ///
    /// This has the ordering and staging contract of [`Self::reserve_child_family_observation`].
    /// Use it for coordinated engine work: the token is checked in the reservation transaction so
    /// an expired or replaced writer cannot allocate durable work. The caller must continue using
    /// that fence when staging and finalizing the selected generation.
    ///
    /// # Errors
    ///
    /// Adds stale or expired lease errors to the unfenced operation's validation and storage
    /// errors. A failed fence leaves the reservation unchanged.
    pub async fn reserve_child_family_observation_fenced(
        &self,
        request: ChildFamilyRequest<'_>,
        token: &ArchiveLeaseToken,
    ) -> Result<FamilyReservation, StoreError> {
        self.reserve_child_family_observation_inner(request, Some(token))
            .await
    }

    /// Reserves a generation before provider pages arrive, under optional fencing.
    async fn reserve_child_family_observation_inner(
        &self,
        request: ChildFamilyRequest<'_>,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<FamilyReservation, StoreError> {
        let ChildFamilyRequest {
            thread,
            family,
            source_clock,
            started_at,
            request_scope,
        } = request;
        if !is_child_family(family) {
            return Err(StoreError::UnsupportedObservationFamily(
                evidence_family_name(family).to_owned(),
            ));
        }
        let request_scope = request_scope.trim();
        if request_scope.is_empty() {
            return Err(StoreError::MissingRequestScope);
        }

        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let source_clock = normalize_source_clock(source_clock)?;
        let source_clock_fields = source_clock_columns(&source_clock)?;
        let family_name = evidence_family_name(family);
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let thread_row_id = thread_row_id(&mut transaction, thread).await?;

        let raw_sequence: i64 = sqlx::query_scalar(
            "UPDATE observation_sequence SET value = value + 1, last_started_at_us = ? WHERE singleton = 1 RETURNING value",
        )
        .bind(started_at.unix_microseconds())
        .fetch_one(&mut *transaction)
        .await?;
        let sequence = checked_sequence(raw_sequence)?;

        let reservation = ReservedGeneration {
            thread: thread_row_id,
            family: family_name,
            clock: source_clock,
            columns: source_clock_fields,
            sequence,
            started_at,
            scope: request_scope,
        };
        let reserved = reservation.supersedes_current(&mut transaction).await?;
        if reserved {
            reservation.persist(&mut transaction).await?;
        }
        transaction.commit().await?;
        Ok(FamilyReservation { sequence, reserved })
    }
}

/// One proposed durable family generation, including the source ordering that selects its owner.
///
/// The sequence has already been allocated in the caller's transaction. Even a rejected proposal
/// commits that allocation, so later acquisition cannot reuse its ordering position. This type
/// performs comparison and persistence on the same connection but never commits independently.
struct ReservedGeneration<'a> {
    /// Validated local parent row used in reservation and generation keys.
    thread: i64,
    /// Persisted name of the selected child evidence family, excluding parent scans.
    family: &'static str,
    /// Normalized source freshness used for ordering against the current reservation.
    clock: SourceClock,
    /// SQL representation of that same clock, reused for both durable records.
    columns: SourceClockColumns,
    /// Already allocated archive-local acquisition order, consumed even if this proposal loses.
    sequence: ObservationSequence,
    /// Local acquisition start recorded independently of source freshness.
    started_at: UtcTimestamp,
    /// Trimmed nonempty request description retained for recovery and inspection.
    scope: &'a str,
}

impl ReservedGeneration<'_> {
    /// Compares source freshness first and acquisition sequence second against the current owner.
    async fn supersedes_current(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<bool, StoreError> {
        let current = sqlx::query(
            "SELECT source_clock_state, source_clock_raw, source_clock_us, sequence FROM thread_family_reservations WHERE thread_id = ? AND family = ?",
        )
        .bind(self.thread)
        .bind(self.family)
        .fetch_optional(&mut *connection)
        .await?;
        if let Some(row) = current {
            let state: String = row.try_get("source_clock_state")?;
            let raw: String = row.try_get("source_clock_raw")?;
            let microseconds: Option<i64> = row.try_get("source_clock_us")?;
            let current_sequence: i64 = row.try_get("sequence")?;
            let current_clock = source_clock_from_columns(&state, &raw, microseconds)?;
            let order = compare_observation_order(
                &self.clock,
                self.sequence,
                &current_clock,
                checked_sequence(current_sequence)?,
            )?;
            return Ok(order == std::cmp::Ordering::Greater);
        }

        Ok(true)
    }

    /// Writes reservation ownership and its recoverable generation record atomically.
    async fn persist(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO thread_family_reservations (thread_id, family, source_clock_state, source_clock_raw, source_clock_us, sequence, started_at_us, request_scope) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (thread_id, family) DO UPDATE SET source_clock_state = excluded.source_clock_state, source_clock_raw = excluded.source_clock_raw, source_clock_us = excluded.source_clock_us, sequence = excluded.sequence, started_at_us = excluded.started_at_us, request_scope = excluded.request_scope",
        )
        .bind(self.thread)
        .bind(self.family)
        .bind(self.columns.state)
        .bind(&self.columns.raw)
        .bind(self.columns.unix_microseconds)
        .bind(to_sql_sequence(self.sequence)?)
        .bind(self.started_at.unix_microseconds())
        .bind(self.scope)
        .execute(&mut *connection)
        .await?;
        sqlx::query(
            "INSERT INTO observation_generations (thread_id, family, sequence, source_clock_state, source_clock_raw, source_clock_us, started_at_us, request_scope, status) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'reserved')",
        )
        .bind(self.thread)
        .bind(self.family)
        .bind(to_sql_sequence(self.sequence)?)
        .bind(self.columns.state)
        .bind(&self.columns.raw)
        .bind(self.columns.unix_microseconds)
        .bind(self.started_at.unix_microseconds())
        .bind(self.scope)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }
}
