use super::{
    Archive, SourceState, SqliteConnectOptions, SqlitePoolOptions, StoreError, ThreadKind,
    apply_thread, discussion, query, remove_archive, repository, temporary_archive_path, thread_id,
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
    apply_thread(
        &archive,
        discussion(
            &thread,
            ThreadKind::Issue,
            SourceState::Open,
            "Migration target",
            Some("backfill searchable body"),
            "2026-09-20T10:00:00Z",
        ),
    )
    .await;
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
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 3")
        .execute(&pool)
        .await
        .expect("mark archive at schema v2");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 4")
        .execute(&pool)
        .await
        .expect("mark archive at schema v2");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 5")
        .execute(&pool)
        .await
        .expect("mark archive at schema v2");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 6")
        .execute(&pool)
        .await
        .expect("mark archive at schema v2");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 7")
        .execute(&pool)
        .await
        .expect("mark archive at schema v2");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 8")
        .execute(&pool)
        .await
        .expect("mark archive at schema v2");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 9")
        .execute(&pool)
        .await
        .expect("mark archive at schema v2");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 10")
        .execute(&pool)
        .await
        .expect("mark archive at schema v2");
    pool.close().await;

    assert!(matches!(
        Archive::open_read_only(&path).await,
        Err(StoreError::MigrationRequired {
            current: 2,
            supported: 10
        })
    ));
    let migration = Archive::migrate(&path).await.expect("migrate archive");
    assert_eq!(migration.applied_migrations.len(), 8);
    let migrated = Archive::open_read_only(&path)
        .await
        .expect("open migrated archive");
    assert_eq!(query(&migrated, "\"backfill\"").await.items.len(), 1);
    migrated.close().await;
    remove_archive(&path);
}
