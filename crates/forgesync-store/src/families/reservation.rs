//! Reserving acquisition order before child-family provider I/O.
//!
//! A reservation gives the later result a stable acquisition position even when provider calls
//! finish out of order. Rejected proposals still consume a sequence but never replace a newer
//! reservation.

use std::cmp::Ordering;

use sqlx::Row;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::families::ChildFamilyRequest;
use crate::families::query::require_child_family;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observations::FamilyReservation;
use crate::ordering::compare_observation_order;
use crate::sql::{
    family_name, normalize_source_clock, sequence_from_sql, source_clock_columns,
    source_clock_from_columns, thread_row_id, to_sql_sequence,
};

impl Archive {
    /// Allocates acquisition order before fetching one discussion's child evidence.
    ///
    /// Call this before provider I/O, then stage pages and finish with the returned sequence.
    /// `reserved == false` still consumes a sequence but leaves the newer reservation intact; do
    /// not fetch or stage that rejected generation.
    pub async fn reserve_child_family_observation_fenced(
        &self,
        request: ChildFamilyRequest<'_>,
        token: &ArchiveLeaseToken,
    ) -> Result<FamilyReservation, StoreError> {
        let ChildFamilyRequest {
            thread,
            family,
            source_clock,
            started_at,
            request_scope,
        } = request;
        require_child_family(family)?;
        let request_scope = request_scope.trim();
        if request_scope.is_empty() {
            return Err(StoreError::MissingRequestScope);
        }

        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let source_clock = normalize_source_clock(source_clock);
        let columns = source_clock_columns(&source_clock);
        let family = family_name(family);
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let thread = thread_row_id(&mut transaction, thread).await?;

        let sequence = sequence_from_sql(
            sqlx::query_scalar(
                "UPDATE observation_sequence SET value = value + 1, last_started_at_us = ? WHERE singleton = 1 RETURNING value",
            )
            .bind(started_at.unix_microseconds())
            .fetch_one(&mut *transaction)
            .await?,
        )?;

        // Source freshness orders first and acquisition sequence second.
        let current = sqlx::query(
            "SELECT source_clock_state, source_clock_raw, source_clock_us, sequence FROM thread_family_reservations WHERE thread_id = ? AND family = ?",
        )
        .bind(thread)
        .bind(family)
        .fetch_optional(&mut *transaction)
        .await?;
        let reserved = match current {
            None => true,
            Some(row) => {
                let current_clock = source_clock_from_columns(
                    &row.try_get::<String, _>("source_clock_state")?,
                    &row.try_get::<String, _>("source_clock_raw")?,
                    row.try_get("source_clock_us")?,
                )?;
                let current_sequence = sequence_from_sql(row.try_get("sequence")?)?;
                compare_observation_order(
                    &source_clock,
                    sequence,
                    &current_clock,
                    current_sequence,
                )? == Ordering::Greater
            }
        };
        if reserved {
            let sql_sequence = to_sql_sequence(sequence)?;
            sqlx::query(
                "INSERT INTO thread_family_reservations (thread_id, family, source_clock_state, source_clock_raw, source_clock_us, sequence, started_at_us, request_scope) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (thread_id, family) DO UPDATE SET source_clock_state = excluded.source_clock_state, source_clock_raw = excluded.source_clock_raw, source_clock_us = excluded.source_clock_us, sequence = excluded.sequence, started_at_us = excluded.started_at_us, request_scope = excluded.request_scope",
            )
            .bind(thread)
            .bind(family)
            .bind(columns.state)
            .bind(&columns.raw)
            .bind(columns.unix_microseconds)
            .bind(sql_sequence)
            .bind(started_at.unix_microseconds())
            .bind(request_scope)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "INSERT INTO observation_generations (thread_id, family, sequence, source_clock_state, source_clock_raw, source_clock_us, started_at_us, request_scope, status) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'reserved')",
            )
            .bind(thread)
            .bind(family)
            .bind(sql_sequence)
            .bind(columns.state)
            .bind(&columns.raw)
            .bind(columns.unix_microseconds)
            .bind(started_at.unix_microseconds())
            .bind(request_scope)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(FamilyReservation { sequence, reserved })
    }
}
