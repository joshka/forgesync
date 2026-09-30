//! # Public cluster restoration and its durable evidence
//!
//! This scenario dismisses and restores a real archived cluster through fenced public operations.
//! It checks the visible dismissal reason, member role/state, representative, and lifecycle before
//! and after restoration. Read-only SQL inspects audit evidence that has no public presentation
//! API.
//!
//! Fixtures construct static domain values only. Archive creation, observation application,
//! generation, lease ownership, dismissal, and restoration remain explicit in the scenario.
//! Source discussions and generated membership must survive both local decisions.
//!
//! The on-disk archive exercises real transaction and writer-fence boundaries. Audit inspection
//! is read-only and follows committed public mutations; it never creates the state being tested.

use std::time::Duration;

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use forgesync_store::clusters::{
    ClusterGenerationInput, ClusterInput, ClusterLifecycle, ClusterListQuery, ClusterMemberInput,
    ClusterMemberRole, ClusterMemberState,
};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Connection, SqliteConnection};

#[tokio::test]
async fn restoration_clears_dismissal_and_preserves_generated_membership() {
    let path = std::env::temp_dir().join(format!(
        "forgesync-cluster-restoration-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let archive = Archive::create(&path).await.expect("create archive");
    let repository = repository();
    let at = UtcTimestamp::parse("2035-01-01T00:00:00Z").expect("timestamp");
    let discussion = discussion(&repository.id, at);
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let sequence = archive
        .reserve_observation_sequence(at)
        .await
        .expect("reserve observation");
    let observation = Observation::new(
        EvidenceFamily::Threads,
        discussion.clone(),
        SourceClock::Valid(at),
        at,
        sequence,
        CollectionCompleteness::Complete,
    );
    archive
        .apply_thread_observation(&observation)
        .await
        .expect("apply discussion");
    let lease = archive
        .acquire_archive_lease(at, Duration::from_secs(3600))
        .await
        .expect("claim writer");
    let generation = ClusterGenerationInput {
        repository: repository.id.clone(),
        endpoint: "https://embeddings.example/v1".to_owned(),
        model: "model-v1".to_owned(),
        recipe: DocumentRecipe::OriginalBody,
        complete_coverage: true,
        eligible_threads: 1,
        vector_threads: 1,
        candidate_edges: 0,
        clusters: vec![ClusterInput {
            representative: discussion.id.clone(),
            title: discussion.title.clone(),
            members: vec![ClusterMemberInput {
                thread: discussion.id.clone(),
                score_to_representative: Some(1.0),
            }],
        }],
    };
    archive
        .save_clusters_fenced(&lease, &generation, at)
        .await
        .expect("save generation");
    let clusters = archive
        .list_clusters(&ClusterListQuery {
            repositories: std::slice::from_ref(&repository.id),
            include_retired: false,
            limit: std::num::NonZeroU32::new(20).expect("limit"),
            offset: 0,
        })
        .await
        .expect("list cluster");
    let id = clusters.items[0].id;

    archive
        .dismiss_cluster_fenced(&lease, id, "  known duplicate  ", at)
        .await
        .expect("dismiss cluster");
    let dismissed = archive.cluster_detail(id).await.expect("dismissed detail");
    assert!(dismissed.cluster.dismissed);
    assert_eq!(
        dismissed.cluster.dismissal_reason.as_deref(),
        Some("known duplicate")
    );

    archive
        .restore_cluster_fenced(&lease, id, at)
        .await
        .expect("restore cluster");
    let restored = archive.cluster_detail(id).await.expect("restored detail");
    assert!(!restored.cluster.dismissed);
    assert!(restored.cluster.dismissal_reason.is_none());
    assert_eq!(restored.cluster.lifecycle, ClusterLifecycle::Active);
    assert_eq!(
        restored.cluster.representative,
        dismissed.cluster.representative
    );
    assert_eq!(restored.cluster.title, dismissed.cluster.title);
    assert_eq!(restored.members, dismissed.members);
    assert_eq!(restored.members.len(), 1);
    assert_eq!(restored.members[0].state, ClusterMemberState::Active);
    assert_eq!(restored.members[0].role, ClusterMemberRole::Representative);

    let options = SqliteConnectOptions::new().filename(&path).read_only(true);
    let mut inspection = SqliteConnection::connect_with(&options)
        .await
        .expect("read-only audit inspection");
    let events: Vec<(String, String)> = sqlx::query_as("SELECT event_type, reason FROM cluster_events WHERE cluster_id = ? AND event_type IN ('dismissed', 'restored') ORDER BY id")
        .bind(i64::try_from(id).expect("cluster row ID")).fetch_all(&mut inspection).await.expect("read decision events");
    assert_eq!(
        events,
        vec![
            ("dismissed".to_owned(), "known duplicate".to_owned()),
            ("restored".to_owned(), String::new())
        ]
    );
    inspection.close().await.expect("close audit inspection");
    archive
        .release_archive_lease(&lease, at)
        .await
        .expect("release writer");
    archive.close().await;
    std::fs::remove_file(path).expect("remove archive");
}

/// Builds only static repository identity/display data; the scenario persists it explicitly.
fn repository() -> Repository {
    Repository {
        id: RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new("repo-restoration").expect("provider ID"),
        ),
        owner: "example".to_owned(),
        name: "restoration".to_owned(),
        full_name: "example/restoration".to_owned(),
        default_branch: None,
        updated_at: None,
        provider_data: Default::default(),
    }
}

/// Builds the single source discussion whose generated membership must survive local decisions.
fn discussion(repository: &RepositoryId, at: UtcTimestamp) -> Discussion {
    Discussion {
        id: ThreadId::new(
            repository.clone(),
            ProviderId::new("thread-1").expect("provider ID"),
            ThreadNumber::new(1).expect("thread number"),
        ),
        kind: ThreadKind::Issue,
        state: SourceState::Open,
        title: "First discussion".to_owned(),
        body: Some("Source content".to_owned()),
        html_url: None,
        created_at: at,
        updated_at: at,
        closed_at: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        provider_data: Default::default(),
    }
}
