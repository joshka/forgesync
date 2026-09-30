//! # Migration-sensitive read cases
//!
//! This regression seeds a current archive with one searchable discussion, closes it, and removes
//! the search index and subsequent schema owners through a raw connection that cannot create a
//! file. It resets migration bookkeeping to version two so explicit migration must rebuild the
//! missing index from retained parent content.
//!
//! The controlled fixture targets search backfill and lifecycle rejection; it is not a complete
//! archived database from an old release. The schema teardown is intentionally visible and linear,
//! keeping the simulated boundary next to the migration operation that consumes it.
//!
//! Read-only opening must first report the exact unsupported schema transition. Migration reports
//! the applied version sequence, then a fresh read-only archive resolves the retained body term to
//! its original discussion identity and title. General lifecycle and corrupt-history cases live in
//! the archive lifecycle suite; this scenario owns preservation of searchable content.

use forgesync_core::content::{SourceState, ThreadKind};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::fixture::{
    discussion, keyword_query, remove_archive, repository, temporary_archive_path, thread_id,
};

#[tokio::test]
async fn explicit_migration_builds_search_index_for_existing_threads() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository("example", "migration", "repo-migration");
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let thread = thread_id(&repository.id, "thread-migration", 4);
    let content = discussion(
        &thread,
        ThreadKind::Issue,
        SourceState::Open,
        "Migration target",
        Some("backfill searchable body"),
        "2026-09-20T10:00:00Z",
    );
    let observed_at = content.updated_at;
    let sequence = archive
        .reserve_observation_sequence(observed_at)
        .await
        .expect("reserve sequence");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        content,
        SourceClock::Valid(observed_at),
        observed_at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply thread observation");
    archive.close().await;

    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(false)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open archive to simulate schema v2");
    sqlx::query("DROP TRIGGER threads_search_insert")
        .execute(&pool)
        .await
        .expect("drop v3 insert trigger");
    sqlx::query("DROP TRIGGER threads_search_delete")
        .execute(&pool)
        .await
        .expect("drop v3 delete trigger");
    sqlx::query("DROP TRIGGER threads_search_update")
        .execute(&pool)
        .await
        .expect("drop v3 update trigger");
    sqlx::query("DROP TABLE thread_search")
        .execute(&pool)
        .await
        .expect("drop v3 search index");
    sqlx::query("DROP TABLE repository_thread_scans")
        .execute(&pool)
        .await
        .expect("drop v4 repository scans");
    sqlx::query("DROP TABLE failures")
        .execute(&pool)
        .await
        .expect("drop v5 failures");
    sqlx::query("DROP TABLE jobs")
        .execute(&pool)
        .await
        .expect("drop v5 jobs");
    sqlx::query("DROP TABLE runs")
        .execute(&pool)
        .await
        .expect("drop v5 runs");
    sqlx::query("DROP TABLE archive_lease")
        .execute(&pool)
        .await
        .expect("drop v5 archive lease");
    sqlx::query("DROP TABLE repository_checkpoints")
        .execute(&pool)
        .await
        .expect("drop v5 checkpoints");
    sqlx::query("DROP TABLE thread_family_head_contexts")
        .execute(&pool)
        .await
        .expect("drop v7 family head context");
    sqlx::query("DROP TABLE cluster_events")
        .execute(&pool)
        .await
        .expect("drop v10 cluster events");
    sqlx::query("DROP TABLE cluster_member_decisions")
        .execute(&pool)
        .await
        .expect("drop v10 cluster decisions");
    sqlx::query("DROP TABLE cluster_memberships")
        .execute(&pool)
        .await
        .expect("drop v10 cluster memberships");
    sqlx::query("DROP TABLE clusters")
        .execute(&pool)
        .await
        .expect("drop v10 clusters");
    sqlx::query("DROP TABLE cluster_runs")
        .execute(&pool)
        .await
        .expect("drop v10 cluster runs");
    sqlx::query("DROP TABLE embeddings")
        .execute(&pool)
        .await
        .expect("drop v9 embeddings");
    sqlx::query("DROP TABLE documents")
        .execute(&pool)
        .await
        .expect("drop v8 documents");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version BETWEEN ? AND ?")
        .bind(3_i64)
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("retain migration bookkeeping through schema v2");
    pool.close().await;

    assert!(matches!(
        Archive::open_read_only(&path).await,
        Err(StoreError::MigrationRequired {
            current: 2,
            supported: 10
        })
    ));
    let migration = Archive::migrate(&path).await.expect("migrate archive");
    assert_eq!(migration.previous_schema_version, 2);
    assert_eq!(migration.schema_version, 10);
    let applied_versions: Vec<_> = migration
        .applied_migrations
        .iter()
        .map(|migration| migration.version)
        .collect();
    assert_eq!(applied_versions, [3, 4, 5, 6, 7, 8, 9, 10]);
    let migrated = Archive::open_read_only(&path)
        .await
        .expect("open migrated archive");
    let results = migrated
        .query_threads(&keyword_query("\"backfill\""))
        .await
        .expect("query keyword page");
    assert_eq!(results.items.len(), 1);
    assert_eq!(results.items[0].discussion.id, thread);
    assert_eq!(results.items[0].discussion.title, "Migration target");
    migrated.close().await;
    remove_archive(&path);
}
