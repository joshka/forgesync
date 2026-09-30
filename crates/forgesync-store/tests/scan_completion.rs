//! # Repository checkpoint completion contract
//!
//! These scenarios use the public archive operations on disk, independently of provider traversal.
//! Completion must reject a remaining page cursor without changing the active scan. Recording the
//! terminal page then permits complete coverage. The test keeps each transition and assertion
//! visible so the transaction boundary can be followed without a behavioral fixture.

use forgesync_core::content::Repository;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId};
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use forgesync_store::enumeration::RepositoryThreadScanStatus;
use forgesync_store::error::StoreError;

#[tokio::test]
async fn completion_requires_a_durably_recorded_terminal_page() {
    let path = std::env::temp_dir().join(format!(
        "forgesync-scan-completion-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository();
    let now = UtcTimestamp::parse("2026-09-20T12:00:00Z").expect("timestamp");
    archive
        .upsert_repository(&repository)
        .await
        .expect("repository");
    let sequence = archive
        .reserve_observation_sequence(now)
        .await
        .expect("reserve scan");
    archive
        .begin_repository_thread_scan(
            &repository.id,
            sequence,
            now,
            "https://api.github.com/page/1",
        )
        .await
        .expect("begin scan");

    let premature = archive
        .finish_repository_thread_scan(
            &repository.id,
            sequence,
            RepositoryThreadScanStatus::Complete,
            now,
            None,
        )
        .await;
    assert!(matches!(
        premature,
        Err(StoreError::InvalidRepositoryThreadScan)
    ));
    let active = archive
        .repository_thread_scan(&repository.id)
        .await
        .expect("read scan")
        .expect("active scan");
    assert_eq!(active.status, RepositoryThreadScanStatus::InProgress);
    assert_eq!(active.pages_completed, 0);
    assert_eq!(
        active.next_page_url.as_deref(),
        Some("https://api.github.com/page/1")
    );

    archive
        .record_repository_thread_scan_page(&repository.id, sequence, 0, None, now)
        .await
        .expect("record empty terminal page");
    archive
        .finish_repository_thread_scan(
            &repository.id,
            sequence,
            RepositoryThreadScanStatus::Complete,
            now,
            None,
        )
        .await
        .expect("finish scan");
    let complete = archive
        .repository_thread_scan(&repository.id)
        .await
        .expect("read scan")
        .expect("complete scan");
    assert_eq!(complete.status, RepositoryThreadScanStatus::Complete);
    assert_eq!(complete.pages_completed, 1);
    assert_eq!(complete.threads_seen, 0);
    assert!(complete.next_page_url.is_none());
    archive.close().await;
    std::fs::remove_file(path).expect("remove closed archive");
}

#[tokio::test]
async fn superseded_generation_cannot_finalize_the_current_scan() {
    let path = std::env::temp_dir().join(format!(
        "forgesync-scan-completion-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository();
    let now = UtcTimestamp::parse("2026-09-20T12:00:00Z").expect("timestamp");
    archive
        .upsert_repository(&repository)
        .await
        .expect("repository");
    let old = archive
        .reserve_observation_sequence(now)
        .await
        .expect("reserve old scan");
    archive
        .begin_repository_thread_scan(&repository.id, old, now, "https://api.github.com/old")
        .await
        .expect("begin old scan");
    archive
        .record_repository_thread_scan_page(&repository.id, old, 1, None, now)
        .await
        .expect("old terminal page");

    let current = archive
        .reserve_observation_sequence(now)
        .await
        .expect("reserve current scan");
    archive
        .begin_repository_thread_scan(
            &repository.id,
            current,
            now,
            "https://api.github.com/current",
        )
        .await
        .expect("supersede old scan");
    let stale = archive
        .finish_repository_thread_scan(
            &repository.id,
            old,
            RepositoryThreadScanStatus::Complete,
            now,
            None,
        )
        .await;
    assert!(matches!(
        stale,
        Err(StoreError::RepositoryThreadScanMissing)
    ));
    let active = archive
        .repository_thread_scan(&repository.id)
        .await
        .expect("read scan")
        .expect("current scan");
    assert_eq!(active.sequence, current);
    assert_eq!(active.status, RepositoryThreadScanStatus::InProgress);
    assert_eq!(active.pages_completed, 0);
    assert_eq!(active.threads_seen, 0);
    assert_eq!(
        active.next_page_url.as_deref(),
        Some("https://api.github.com/current")
    );
    assert!(active.failure.is_none());
    archive.close().await;
    std::fs::remove_file(path).expect("remove closed archive");
}

/// Builds static repository data; each scenario performs its own archive transitions explicitly.
fn repository() -> Repository {
    Repository {
        id: RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new("41").expect("provider ID"),
        ),
        owner: "owner".to_owned(),
        name: "repo".to_owned(),
        full_name: "owner/repo".to_owned(),
        default_branch: None,
        updated_at: None,
        provider_data: Default::default(),
    }
}
