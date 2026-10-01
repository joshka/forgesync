//! Aggregate archive counts and family coverage totals.
//!
//! Missing coverage rows count as missing evidence; member counts never substitute for a recorded
//! complete state. Pull-request-only families exclude issues from their denominator.

use forgesync_core::identity::RepositoryId;
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::archive::Archive;
use crate::error::StoreError;
use crate::reads::{ArchiveStatus, FamilyCoverageSummary};
use crate::sql::{
    ALL_FAMILIES, count_from_sql, family_name, is_pull_request_family, push_repository_scope,
};

impl Archive {
    /// Returns coverage counts for all families, optionally limited to resolved repositories.
    pub async fn coverage_summary(
        &self,
        repositories: &[RepositoryId],
    ) -> Result<Vec<FamilyCoverageSummary>, StoreError> {
        // Family labels are fixed identifiers, so inlining them in the CTE binds no user input.
        let families = ALL_FAMILIES
            .into_iter()
            .map(|family| {
                format!(
                    "('{}', {})",
                    family_name(family),
                    u8::from(is_pull_request_family(family))
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let mut statement = QueryBuilder::<Sqlite>::new(format!(
            "WITH families (name, pull_request_only) AS (VALUES {families}) SELECT f.name AS family, COALESCE(c.status, 'missing') AS status, COUNT(*) AS item_count FROM families f JOIN threads t ON f.pull_request_only = 0 OR t.kind = 'pull_request' JOIN repositories r ON r.id = t.repository_id LEFT JOIN family_coverage c ON c.thread_id = t.id AND c.family = f.name WHERE 1 = 1"
        ));
        push_repository_scope(&mut statement, repositories);
        statement.push(" GROUP BY f.name, COALESCE(c.status, 'missing')");
        let rows = statement.build().fetch_all(&self.reader).await?;
        let mut summaries = ALL_FAMILIES
            .into_iter()
            .map(|family| FamilyCoverageSummary {
                family,
                applicable_threads: 0,
                missing: 0,
                incomplete: 0,
                complete: 0,
            })
            .collect::<Vec<_>>();
        for row in rows {
            let family: String = row.try_get("family")?;
            let summary = summaries
                .iter_mut()
                .find(|summary| family_name(summary.family) == family)
                .ok_or(StoreError::Corrupt("archive_coverage_invalid"))?;
            let status: String = row.try_get("status")?;
            summary.record_count(&status, count_from_sql(row.try_get("item_count")?)?)?;
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
            let count = count_from_sql(row.try_get("item_count")?)?;
            match kind.as_str() {
                "issue" => issues = count,
                "pull_request" => pull_requests = count,
                _ => return Err(StoreError::Corrupt("archive_thread_kind_invalid")),
            }
        }
        let threads = issues
            .checked_add(pull_requests)
            .ok_or(StoreError::IntegerOutOfRange)?;
        Ok(ArchiveStatus {
            archive: self.info().clone(),
            repositories: count_from_sql(repositories)?,
            threads,
            issues,
            pull_requests,
            coverage: self.coverage_summary(&[]).await?,
            diagnostics: self.diagnostics().await?,
        })
    }
}

impl FamilyCoverageSummary {
    /// Accumulates one grouped status count into its bucket and the denominator.
    fn record_count(&mut self, status: &str, count: u64) -> Result<(), StoreError> {
        self.applicable_threads = self
            .applicable_threads
            .checked_add(count)
            .ok_or(StoreError::IntegerOutOfRange)?;
        match status {
            "missing" => self.missing = count,
            "incomplete" => self.incomplete = count,
            "complete" => self.complete = count,
            _ => return Err(StoreError::Corrupt("archive_coverage_invalid")),
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
    fn unknown_stored_status_is_rejected() {
        let mut summary = empty_summary();
        assert!(matches!(
            summary.record_count("unexpected", 1),
            Err(StoreError::Corrupt("archive_coverage_invalid"))
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
