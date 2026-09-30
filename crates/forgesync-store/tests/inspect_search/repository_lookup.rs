//! # Case-insensitive repository resolution
//!
//! Two registered repository names distinguish the requested identity from another valid row.
//! Lookup uses uppercase display coordinates while stored metadata retains lowercase spelling.
//! The expected host and repository identity are checked values supplied directly.
//! No discussions or full-text fixtures are needed to establish repository resolution.
//!
//! Archive creation and registration remain visible in the scenario.
//! Construction helpers execute no archive operation and calculate no expected lookup result.
//! Missing repository and CLI selector policy belong to separate boundary suites.
//! This case checks the public store lookup and closes the archive before cleanup.

use forgesync_core::identity::GitHubHost;
use forgesync_store::archive::Archive;

use crate::fixture::{remove_archive, repository, temporary_archive_path};

#[tokio::test]
async fn repository_lookup_ignores_display_case() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let first_repository = repository("example", "first", "repo-first");
    let second_repository = repository("example", "second", "repo-second");
    archive
        .upsert_repository(&first_repository)
        .await
        .expect("store first repository");
    archive
        .upsert_repository(&second_repository)
        .await
        .expect("store second repository");

    let found_repository = archive
        .find_repository(
            &GitHubHost::parse("github.com").expect("host"),
            "EXAMPLE",
            "FIRST",
        )
        .await
        .expect("repository lookup");
    let found_repository = found_repository.expect("case-insensitive repository lookup");
    assert_eq!(found_repository.id, first_repository.id);

    archive.close().await;
    remove_archive(&path);
}
