//! Child-family reservation operations.

use sqlx::Row;

use super::{
    Archive, ArchiveLeaseToken, EvidenceFamily, FamilyReservation, SourceClock, StoreError,
    ThreadId, UtcTimestamp, checked_sequence, compare_observation_order, evidence_family_name,
    is_child_family, normalize_source_clock, require_active_archive_lease, source_clock_columns,
    source_clock_from_columns, thread_row_id, to_sql_sequence,
};

impl Archive {
    /// Allocates a sequence and tries to reserve an independently ordered child family.
    pub async fn reserve_child_family_observation(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        source_clock: &SourceClock,
        started_at: UtcTimestamp,
        request_scope: &str,
    ) -> Result<FamilyReservation, StoreError> {
        self.reserve_child_family_observation_inner(
            thread,
            family,
            source_clock,
            started_at,
            request_scope,
            None,
        )
        .await
    }

    /// Reserves a child family only while the supplied archive lease remains current.
    pub async fn reserve_child_family_observation_fenced(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        source_clock: &SourceClock,
        started_at: UtcTimestamp,
        request_scope: &str,
        token: &ArchiveLeaseToken,
    ) -> Result<FamilyReservation, StoreError> {
        self.reserve_child_family_observation_inner(
            thread,
            family,
            source_clock,
            started_at,
            request_scope,
            Some(token),
        )
        .await
    }

    /// Reserves a generation before provider pages arrive, under optional fencing.
    async fn reserve_child_family_observation_inner(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        source_clock: &SourceClock,
        started_at: UtcTimestamp,
        request_scope: &str,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<FamilyReservation, StoreError> {
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

        let current = sqlx::query(
            "SELECT source_clock_state, source_clock_raw, source_clock_us, sequence FROM thread_family_reservations WHERE thread_id = ? AND family = ?",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(row) = current {
            let state: String = row.try_get("source_clock_state")?;
            let raw: String = row.try_get("source_clock_raw")?;
            let microseconds: Option<i64> = row.try_get("source_clock_us")?;
            let current_sequence: i64 = row.try_get("sequence")?;
            let current_clock = source_clock_from_columns(&state, &raw, microseconds)?;
            let order = compare_observation_order(
                &source_clock,
                sequence,
                &current_clock,
                checked_sequence(current_sequence)?,
            )?;
            if order != std::cmp::Ordering::Greater {
                transaction.commit().await?;
                return Ok(FamilyReservation {
                    sequence,
                    reserved: false,
                });
            }
        }

        sqlx::query(
            "INSERT INTO thread_family_reservations (thread_id, family, source_clock_state, source_clock_raw, source_clock_us, sequence, started_at_us, request_scope) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (thread_id, family) DO UPDATE SET source_clock_state = excluded.source_clock_state, source_clock_raw = excluded.source_clock_raw, source_clock_us = excluded.source_clock_us, sequence = excluded.sequence, started_at_us = excluded.started_at_us, request_scope = excluded.request_scope",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .bind(source_clock_fields.state)
        .bind(&source_clock_fields.raw)
        .bind(source_clock_fields.unix_microseconds)
        .bind(to_sql_sequence(sequence)?)
        .bind(started_at.unix_microseconds())
        .bind(request_scope)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO observation_generations (thread_id, family, sequence, source_clock_state, source_clock_raw, source_clock_us, started_at_us, request_scope, status) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'reserved')",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .bind(to_sql_sequence(sequence)?)
        .bind(source_clock_fields.state)
        .bind(&source_clock_fields.raw)
        .bind(source_clock_fields.unix_microseconds)
        .bind(started_at.unix_microseconds())
        .bind(request_scope)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(FamilyReservation {
            sequence,
            reserved: true,
        })
    }
}
