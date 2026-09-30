//! # Aggregate archive counts and evidence coverage
//!
//! `Archive::coverage_summary` counts explicit family coverage states across the selected
//! repository scope. Pull-request-only families exclude issues from their denominator. Missing
//! coverage rows contribute to missing evidence; member counts never substitute for a recorded
//! complete state.
//!
//! `Archive::archive_status` combines repository/thread totals, family summaries, and diagnostics.
//! These operations read the archive without refreshing sources or rewriting evidence. Each query
//! observes its own read snapshot; the assembled status is diagnostic rather than one
//! transactionally frozen view of concurrent writers.
//!
//! `FamilyCoverageSummary::record_count` owns checked accumulation and stored-status validation.
//! Individual discussion coverage and stale-head projection remain in `coverage`.

use forgesync_core::identity::RepositoryId;
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::archive::Archive;
use crate::coverage_projection::{ALL_FAMILIES, evidence_family_name, is_pull_request_family};
use crate::error::StoreError;
use crate::reads::query::push_repository_scope;
use crate::reads::{ArchiveStatus, FamilyCoverageSummary};

impl Archive {
    /// Returns coverage counts for all families, optionally limited to resolved repositories.
    pub async fn coverage_summary(
        &self,
        repositories: &[RepositoryId],
    ) -> Result<Vec<FamilyCoverageSummary>, StoreError> {
        let mut summaries = Vec::with_capacity(ALL_FAMILIES.len());
        for family in ALL_FAMILIES {
            let mut statement = QueryBuilder::<Sqlite>::new(
                "SELECT COALESCE(c.status, 'missing') AS status, COUNT(*) AS item_count FROM threads t JOIN repositories r ON r.id = t.repository_id LEFT JOIN family_coverage c ON c.thread_id = t.id AND c.family = ",
            );
            statement
                .push_bind(evidence_family_name(family))
                .push(" WHERE 1 = 1");
            push_repository_scope(&mut statement, repositories);
            if is_pull_request_family(family) {
                statement.push(" AND t.kind = 'pull_request'");
            }
            statement.push(" GROUP BY COALESCE(c.status, 'missing')");
            let rows = statement.build().fetch_all(&self.reader).await?;
            let mut summary = FamilyCoverageSummary {
                family,
                applicable_threads: 0,
                missing: 0,
                incomplete: 0,
                complete: 0,
            };
            for row in rows {
                let status: String = row.try_get("status")?;
                let count: i64 = row.try_get("item_count")?;
                summary.record_count(&status, count)?;
            }
            summaries.push(summary);
        }
        Ok(summaries)
    }

    /// Returns local archive counts and aggregate per-family coverage.
    pub async fn archive_status(&self) -> Result<ArchiveStatus, StoreError> {
        let repositories: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repositories")
            .fetch_one(&self.reader)
            .await?;
        let rows = sqlx::query("SELECT kind, COUNT(*) AS item_count FROM threads GROUP BY kind")
            .fetch_all(&self.reader)
            .await?;
        let mut issues = 0_u64;
        let mut pull_requests = 0_u64;
        for row in rows {
            let kind: String = row.try_get("kind")?;
            let count: i64 = row.try_get("item_count")?;
            let count = u64::try_from(count).map_err(|_| StoreError::InvalidStoredCount)?;
            match kind.as_str() {
                "issue" => issues = count,
                "pull_request" => pull_requests = count,
                _ => return Err(StoreError::InvalidStoredThreadKind(kind)),
            }
        }
        let repositories =
            u64::try_from(repositories).map_err(|_| StoreError::InvalidStoredCount)?;
        let threads = issues
            .checked_add(pull_requests)
            .ok_or(StoreError::IntegerOutOfRange)?;
        Ok(ArchiveStatus {
            archive: self.info().clone(),
            repositories,
            threads,
            issues,
            pull_requests,
            coverage: self.coverage_summary(&[]).await?,
            diagnostics: self.diagnostics().await?,
        })
    }
}

impl FamilyCoverageSummary {
    /// Accumulates one grouped SQL count and rejects unknown coverage labels or invalid counts.
    ///
    /// SQL produces one row per status, so each bucket is assigned once. The denominator sums
    /// every bucket with checked arithmetic before the summary becomes visible to callers.
    fn record_count(&mut self, status: &str, count: i64) -> Result<(), StoreError> {
        let count = u64::try_from(count).map_err(|_| StoreError::InvalidStoredCount)?;
        self.applicable_threads = self
            .applicable_threads
            .checked_add(count)
            .ok_or(StoreError::IntegerOutOfRange)?;
        match status {
            "missing" => self.missing = count,
            "incomplete" => self.incomplete = count,
            "complete" => self.complete = count,
            _ => return Err(StoreError::InvalidStoredCoverage),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use forgesync_core::coverage::EvidenceFamily;

    use crate::error::StoreError;
    use crate::reads::FamilyCoverageSummary;

    #[test]
    fn grouped_coverage_counts_keep_each_bucket_and_the_denominator() {
        let mut summary = empty_summary();
        summary.record_count("missing", 2).expect("missing count");
        summary
            .record_count("incomplete", 3)
            .expect("incomplete count");
        summary.record_count("complete", 5).expect("complete count");
        assert_eq!(summary.applicable_threads, 10);
        assert_eq!(summary.missing, 2);
        assert_eq!(summary.incomplete, 3);
        assert_eq!(summary.complete, 5);
    }

    #[test]
    fn negative_stored_count_is_rejected() {
        let mut summary = empty_summary();
        assert!(matches!(
            summary.record_count("complete", -1),
            Err(StoreError::InvalidStoredCount)
        ));
    }

    #[test]
    fn unknown_stored_status_is_rejected() {
        let mut summary = empty_summary();
        assert!(matches!(
            summary.record_count("unexpected", 1),
            Err(StoreError::InvalidStoredCoverage)
        ));
    }

    /// Builds static zero counts without performing the accumulation under test.
    fn empty_summary() -> FamilyCoverageSummary {
        FamilyCoverageSummary {
            family: EvidenceFamily::Comments,
            applicable_threads: 0,
            missing: 0,
            incomplete: 0,
            complete: 0,
        }
    }
}
