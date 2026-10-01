//! # Kind and state select the closed pull request
//!
//! Two repositories and three explicit observations distinguish scope, kind, state, and timestamps.
//! One issue and one pull request belong to the selected repository; another issue belongs
//! elsewhere. Each sequence reservation and application is visible rather than hidden in a behavior
//! fixture. This scenario isolates the named read contract from independent lookup and query
//! validation.
//!
//! Payload helpers construct values without archive writes or expected-result calculations.
//! Reads operate on the on-disk archive after complete parent evidence has been committed.
//! Engine policy and CLI rendering remain outside this store projection contract.
//! Cleanup follows explicit archive closure.

use std::num::NonZeroU32;

use forgesync_core::content::{SourceState, ThreadKind};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_store::archive::Archive;
use forgesync_store::reads::{ThreadQuery, ThreadSort, ThreadStateFilter};

use crate::fixture::{discussion, remove_archive, repository, temporary_archive_path, thread_id};

#[tokio::test]
async fn kind_and_state_select_the_closed_pull_request() {
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

    let first_thread = thread_id(&first_repository.id, "thread-1", 1);
    let second_thread = thread_id(&first_repository.id, "thread-2", 2);
    let third_thread = thread_id(&second_repository.id, "thread-3", 3);
    let content = discussion(
        &first_thread,
        ThreadKind::Issue,
        SourceState::Open,
        "Needle in title",
        Some("body text"),
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
        .apply_thread_observation(&observation, None)
        .await
        .expect("apply thread observation");
    let content = discussion(
        &second_thread,
        ThreadKind::PullRequest,
        SourceState::Closed,
        "Other title",
        Some("needle in discussion"),
        "2026-09-20T10:00:02Z",
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
        .apply_thread_observation(&observation, None)
        .await
        .expect("apply thread observation");
    let content = discussion(
        &third_thread,
        ThreadKind::Issue,
        SourceState::Open,
        "Needle elsewhere",
        None,
        "2026-09-20T10:00:01Z",
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
        .apply_thread_observation(&observation, None)
        .await
        .expect("apply thread observation");

    let closed = archive
        .query_threads(&ThreadQuery {
            repositories: vec![first_repository.id.clone()],
            kind: Some(ThreadKind::PullRequest),
            state: ThreadStateFilter::Closed,
            match_expression: None,
            updated_since: None,
            sort: ThreadSort::Created,
            limit: NonZeroU32::new(10).expect("positive limit"),
            offset: 0,
        })
        .await
        .expect("filter closed pull requests");
    assert_eq!(closed.items.len(), 1);
    assert_eq!(closed.items[0].discussion.id.number().get(), 2);

    archive.close().await;
    remove_archive(&path);
}
