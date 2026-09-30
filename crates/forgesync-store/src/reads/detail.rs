//! # Assemble an offline thread detail
//!
//! `Archive` detail methods combine a canonical discussion with timeline events, child resources,
//! and coverage. The resulting `ThreadDetail` is the store-to-engine read boundary for inspection
//! and document building.
//!
//! This module translates rows and ordering into a coherent view. Consumers should use the
//! projection to retain selected membership, coverage, and the established timeline ordering.
//! `timeline` owns event projection; this module owns row decoding and evidence assembly.
//!
//! These queries run as separate archive reads rather than a single frozen read transaction.
//! Concurrent writers can advance evidence between reads. The result is an inspection view, not
//! proof that every included family was acquired at the same time; its coverage remains explicit.

use forgesync_core::content::{Comment, Discussion, Review, ReviewThread};
use forgesync_core::identity::ThreadReference;
use serde::de::DeserializeOwned;
use sqlx::Row;

use crate::archive::Archive;
use crate::coverage_projection::{coverage_for_kind, load_thread_coverage};
use crate::error::StoreError;
use crate::observations::StagedItem;
use crate::reads::timeline::thread_timeline;
use crate::reads::{ThreadDetail, ThreadSummary};

impl Archive {
    /// Returns current thread details and typed selected evidence for a resolved reference.
    pub async fn thread_detail(
        &self,
        reference: &ThreadReference,
    ) -> Result<ThreadDetail, StoreError> {
        let row = sqlx::query(
            "SELECT t.id, r.payload_json AS repository_json, t.payload_json AS discussion_json FROM threads t JOIN repositories r ON r.id = t.repository_id WHERE r.host = ? AND r.provider_id = ? AND t.number = ?",
        )
        .bind(reference.repository().host().as_str())
        .bind(reference.repository().provider_id().as_str())
        .bind(i64::try_from(reference.number().get()).map_err(|_| StoreError::IntegerOutOfRange)?)
        .fetch_optional(&self.reader)
        .await?
        .ok_or(StoreError::ThreadMissing)?;
        let row_id: i64 = row.try_get("id")?;
        let repository_json: String = row.try_get("repository_json")?;
        let discussion_json: String = row.try_get("discussion_json")?;
        let repository = serde_json::from_str(&repository_json)?;
        let discussion: Discussion = serde_json::from_str(&discussion_json)?;
        let coverage_by_thread = load_thread_coverage(&self.reader, &[row_id]).await?;
        let summary = ThreadSummary {
            repository,
            coverage: coverage_for_kind(&discussion, coverage_by_thread.get(&row_id)),
            discussion,
        };

        let comments: Vec<StagedItem<Comment>> =
            load_family_members(&self.reader, row_id, "comments").await?;
        let pull_request_metadata =
            load_family_members(&self.reader, row_id, "pull_request_metadata").await?;
        let reviews: Vec<StagedItem<Review>> =
            load_family_members(&self.reader, row_id, "reviews").await?;
        let review_threads: Vec<StagedItem<ReviewThread>> =
            load_family_members(&self.reader, row_id, "review_threads").await?;
        let timeline = thread_timeline(&summary.discussion, &comments, &reviews, &review_threads);

        Ok(ThreadDetail {
            summary,
            comments,
            pull_request_metadata,
            reviews,
            review_threads,
            timeline,
        })
    }
}

/// Loads current complete family membership for a discussion.
async fn load_family_members<T>(
    pool: &sqlx::SqlitePool,
    thread_id: i64,
    family: &str,
) -> Result<Vec<StagedItem<T>>, StoreError>
where
    T: DeserializeOwned,
{
    let rows = sqlx::query(
        "SELECT provider_id, payload_json FROM thread_family_membership WHERE thread_id = ? AND family = ? ORDER BY provider_id",
    )
    .bind(thread_id)
    .bind(family)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let id: String = row.try_get("provider_id")?;
            let payload_json: String = row.try_get("payload_json")?;
            let id = forgesync_core::identity::ProviderId::new(id)
                .map_err(|_| StoreError::InvalidStoredProviderId)?;
            Ok(StagedItem {
                id,
                payload: serde_json::from_str(&payload_json)?,
            })
        })
        .collect()
}
