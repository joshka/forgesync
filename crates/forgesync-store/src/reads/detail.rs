//! Offline thread detail: canonical discussion, selected child evidence, timeline, and coverage.
//!
//! Sections are separate reads, so the result is an inspection view rather than proof that every
//! family was acquired at the same time; coverage stays explicit.

use forgesync_core::content::{Comment, Discussion, Review, ReviewThread};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::ThreadReference;
use sqlx::Row;

use crate::archive::Archive;
use crate::coverage_projection::{coverage_for_kind, load_thread_coverage};
use crate::error::StoreError;
use crate::families::query::family_members;
use crate::observations::StagedItem;
use crate::reads::timeline::thread_timeline;
use crate::reads::{ThreadDetail, ThreadSummary};
use crate::sql::to_sql_integer;

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
        .bind(to_sql_integer(reference.number().get())?)
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
            family_members(&self.reader, row_id, EvidenceFamily::Comments).await?;
        let pull_request_metadata =
            family_members(&self.reader, row_id, EvidenceFamily::PullRequestMetadata).await?;
        let reviews: Vec<StagedItem<Review>> =
            family_members(&self.reader, row_id, EvidenceFamily::Reviews).await?;
        let review_threads: Vec<StagedItem<ReviewThread>> =
            family_members(&self.reader, row_id, EvidenceFamily::ReviewThreads).await?;
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
