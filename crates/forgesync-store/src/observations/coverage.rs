//! # Read directly recorded family coverage
//!
//! [`Archive::family_coverage`] looks up a canonical discussion by durable identity, then reads
//! the stored completeness state for one evidence family. An existing discussion without a family
//! row returns [`CoverageState::Missing`]; a missing discussion fails identity lookup instead.
//! Malformed stored state JSON is an error, not a missing-state fallback.
//!
//! This is a direct recorded-state read. It wraps the decoded state in [`Coverage`] without
//! comparing source timestamps or pull-request head context, so it does not derive staleness.
//! The richer read projections own those comparisons when presenting retained discussion evidence.
//! Callers should choose that projection when they need to judge whether recorded completeness is
//! still current rather than only inspecting the persisted family ledger.
//!
//! Identity lookup and the coverage query share one connection, not a transaction snapshot.
//! This operation writes no coverage, replaces no membership, and creates no observation. Family
//! acquisition and observation application own those mutations; the presence of child content
//! alone does not establish a complete collection.

use forgesync_core::coverage::{Coverage, CoverageState, EvidenceFamily};
use forgesync_core::identity::ThreadId;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::observation_sql::{evidence_family_name, thread_row_id};

impl Archive {
    /// Reads the stored family state for an existing canonical discussion.
    ///
    /// An absent family row returns `Missing`; an absent discussion or invalid stored JSON fails.
    /// The returned coverage does not derive staleness from source timestamps or head context.
    /// Identity resolution and the state query are separate reads, not a frozen snapshot.
    pub async fn family_coverage(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
    ) -> Result<Coverage, StoreError> {
        let family_name = evidence_family_name(family);
        let mut connection = self.reader.acquire().await?;
        let thread_row_id = thread_row_id(&mut connection, thread).await?;
        let state_json: Option<String> = sqlx::query_scalar(
            "SELECT state_json FROM family_coverage WHERE thread_id = ? AND family = ?",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .fetch_optional(&mut *connection)
        .await?;
        let state = match state_json {
            Some(json) => serde_json::from_str(&json)?,
            None => CoverageState::Missing,
        };
        Ok(Coverage::new(family, state))
    }
}

#[cfg(test)]
mod tests {
    //! A single-reader archive proves coverage lookup never needs a nested pool acquisition.
    //! Minimal SQL rows establish only identity and missing coverage; payload projection is outside
    //! this test. The bounded timeout detects connection starvation without waiting for SQLx's
    //! default pool deadline. The archive uses a real on-disk database and is closed before
    //! removal.

    use std::time::Duration;

    use forgesync_core::coverage::{CoverageState, EvidenceFamily};
    use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    use crate::archive::Archive;

    #[tokio::test]
    async fn coverage_read_uses_only_one_connection() {
        let directory = std::env::temp_dir().join(format!(
            "forgesync-coverage-connection-{}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).expect("create fixture directory");
        let path = directory.join("archive.sqlite");
        let mut archive = Archive::create(&path).await.expect("create archive");
        let writer = archive.writer.as_ref().expect("writable archive");
        let repository_row = sqlx::query(
            "INSERT INTO repositories (id, host, provider_id, owner, name, full_name, provider_data_json, payload_json) VALUES (1, 'github.com', 'repo', 'example', 'repo', 'example/repo', '{}', '{}')",
        );
        repository_row
            .execute(writer)
            .await
            .expect("insert repository identity");
        let thread_row = sqlx::query(
            "INSERT INTO threads (repository_id, provider_id, number, kind, state, title, created_at_us, updated_at_us, labels_json, assignees_json, provider_data_json, payload_json, source_clock_state, source_clock_raw, observation_sequence, observed_at_us, evidence_clock_state, evidence_clock_raw) VALUES (1, 'thread', 1, 'issue', 'open', 'Fixture', 0, 0, '[]', '[]', '{}', '{}', 'missing', '', 1, 0, 'missing', '')",
        );
        thread_row
            .execute(writer)
            .await
            .expect("insert thread identity");
        archive.reader.close().await;
        archive.reader = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::new().filename(&path).read_only(true))
            .await
            .expect("single reader connection");
        let repository = RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new("repo").expect("repository ID"),
        );
        let thread = ThreadId::new(
            repository,
            ProviderId::new("thread").expect("thread ID"),
            ThreadNumber::new(1).expect("number"),
        );

        let coverage = tokio::time::timeout(
            Duration::from_secs(1),
            archive.family_coverage(&thread, EvidenceFamily::Comments),
        )
        .await
        .expect("read completes with one connection")
        .expect("read coverage");

        assert_eq!(
            coverage,
            forgesync_core::coverage::Coverage::new(
                EvidenceFamily::Comments,
                CoverageState::Missing
            )
        );
        archive.close().await;
        std::fs::remove_dir_all(directory).expect("remove fixture directory");
    }
}
