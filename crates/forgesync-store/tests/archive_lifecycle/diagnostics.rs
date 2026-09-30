//! # Read-only diagnostics over explicit ledger facts
//!
//! Healthy probes must leave no persistent helper tables. Explicit failed/deferred SQL jobs
//! establish diagnostic buckets while a real writer lease establishes held-owner reporting.
//!
//! Each case invokes archive operations directly and closes handles before cleanup.
//! Shared infrastructure allocates filenames and opens raw pools; it executes no scenario.
//! SQLite is on disk so file effects, migration history, and pool behavior are observable.
//! SQL mutation is fixture arrangement rather than a supported application write path.
//! Provider acquisition, source membership, and engine retry are outside this lifecycle suite.
//! Failure assertions name the store variant or retained diagnostic facts under examination.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;

use crate::fixture::{read_only_pool, remove_archive, temporary_archive_path, writable_pool};

#[tokio::test]
async fn healthy_diagnostics_leave_no_persistent_probe_tables() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    archive.close().await;
    let archive = Archive::open_read_only(&path)
        .await
        .expect("open read-only archive");

    let doctor = archive.doctor().await.expect("read archive diagnostics");
    assert!(doctor.healthy, "doctor report: {doctor:?}");
    assert_eq!(doctor.checks.len(), 4);
    assert!(doctor.checks.iter().all(|check| check.healthy));
    assert!(doctor.diagnostics.schema.history_valid);
    assert_eq!(
        doctor.diagnostics.schema.current_version,
        doctor.diagnostics.schema.supported_version
    );
    assert!(!doctor.diagnostics.lease.held);
    assert_eq!(doctor.diagnostics.work.unresolved_failures, 0);
    let before_status = archive.archive_status().await.expect("read status");
    assert_eq!(before_status.diagnostics.work.failed_jobs, 0);
    assert_eq!(before_status.diagnostics.work.deferred_jobs, 0);
    archive.close().await;

    let pool = read_only_pool(&path).await;
    let persisted_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name LIKE '__forgesync_%'",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect tables");
    assert_eq!(persisted_tables, 0, "doctor probes must remain temporary");
    pool.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn diagnostics_report_schema_lease_and_unresolved_family_work_read_only() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    archive.close().await;

    let pool = writable_pool(&path).await;
    let repository_payload = serde_json::json!({
        "id": { "host": "github.com", "provider_id": "41" },
        "owner": "owner",
        "name": "repo",
        "full_name": "owner/repo",
        "default_branch": null,
        "updated_at": null,
        "provider_data": {}
    });
    let repository_id: i64 = sqlx::query_scalar(
        "INSERT INTO repositories (host, provider_id, owner, name, full_name, provider_data_json, payload_json) VALUES ('github.com', '41', 'owner', 'repo', 'owner/repo', '{}', ?) RETURNING id",
    )
    .bind(repository_payload.to_string())
    .fetch_one(&pool)
    .await
    .expect("insert repository fixture");
    let run_id: i64 = sqlx::query_scalar(
        "INSERT INTO runs (status, started_at_us, updated_at_us, scope_json) VALUES ('partial', 1000, 1000, '{}') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert run fixture");
    sqlx::query(
        "INSERT INTO jobs (run_id, repository_id, family, scope_key, status, started_at_us, updated_at_us) VALUES (?, ?, 'comments', 'open', 'failed', 1000, 1000), (?, ?, 'reviews', 'open', 'deferred', 1000, 1000)",
    )
    .bind(run_id)
    .bind(repository_id)
    .bind(run_id)
    .bind(repository_id)
    .execute(&pool)
    .await
    .expect("insert failed comments and deferred reviews jobs");
    sqlx::query(
        "INSERT INTO failures (run_id, job_id, repository_id, family, target_key, scope_key, failure_json, created_at_us) SELECT run_id, id, repository_id, family, 'owner/repo', 'open', '{\"kind\":\"network\",\"message\":\"fixture failure\"}', 1000 FROM jobs WHERE run_id = ?",
    )
    .bind(run_id)
    .execute(&pool)
    .await
    .expect("attach unresolved failure evidence to both jobs");
    pool.close().await;

    let archive = Archive::open_read_write(&path)
        .await
        .expect("open archive for lease");
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after Unix epoch");
    let now = UtcTimestamp::from_unix_microseconds(
        i64::try_from(elapsed.as_micros()).expect("timestamp fits archive"),
    )
    .expect("valid timestamp");
    let lease = archive
        .acquire_archive_lease(now, std::time::Duration::from_secs(60))
        .await
        .expect("acquire archive lease");
    let pool = read_only_pool(&path).await;
    let owner: String =
        sqlx::query_scalar("SELECT owner_id FROM archive_lease WHERE singleton = 1")
            .fetch_one(&pool)
            .await
            .expect("read acquired owner");
    pool.close().await;
    let readonly = Archive::open_read_only(&path)
        .await
        .expect("open read-only archive while leased");
    let diagnostics = readonly.diagnostics().await.expect("read diagnostics");
    assert!(diagnostics.schema.history_valid);
    assert!(diagnostics.lease.held);
    assert_eq!(diagnostics.lease.owner_id, Some(owner));
    assert_eq!(diagnostics.lease.fencing_token, 1);
    assert_eq!(
        diagnostics.lease.expires_at.unix_microseconds(),
        now.unix_microseconds() + 60_000_000
    );
    assert_eq!(diagnostics.work.failed_jobs, 1);
    assert_eq!(diagnostics.work.deferred_jobs, 1);
    assert_eq!(diagnostics.work.unresolved_failures, 2);
    let family_counts = diagnostics
        .work
        .failures_by_family
        .iter()
        .map(|count| (count.family, count.unresolved))
        .collect::<Vec<_>>();
    assert_eq!(
        family_counts,
        [
            (EvidenceFamily::Threads, 0),
            (EvidenceFamily::Comments, 1),
            (EvidenceFamily::PullRequestMetadata, 0),
            (EvidenceFamily::Reviews, 1),
            (EvidenceFamily::ReviewThreads, 0),
        ]
    );
    assert_eq!(diagnostics.work.unassigned_failures, 0);
    assert_eq!(diagnostics.work.in_progress_runs, 0);
    assert!(readonly.doctor().await.expect("doctor").healthy);
    readonly.close().await;
    archive
        .release_archive_lease(&lease, now)
        .await
        .expect("release archive lease");
    archive.close().await;
    remove_archive(&path);
}
