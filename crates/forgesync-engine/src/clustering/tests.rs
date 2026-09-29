//! # Cluster candidate policy
//!
//! These tests document safeguards in the candidate graph: weak title evidence, cross-kind
//! relationships, repository-scoped references, fanout, and maximum component size. The exact
//! fixture relationships matter because transitive grouping can make a plausible pair produce an
//! implausible cluster. Read these alongside `candidates` before changing thresholds or union
//! behavior; a new candidate rule should have a small example that explains the intended grouping.

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::coverage::Coverage;
use forgesync_core::embedding::EmbeddingVector;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::embeddings::{EmbeddingSearchDocument, StoredEmbeddingChunk};
use forgesync_store::reads::ThreadSummary;
use tokio_util::sync::CancellationToken;

use super::ClusterOptions;
use super::candidates::build_cluster_candidates;
use super::components::ClusterCandidate;

#[test]
fn cluster_graph_applies_weak_title_and_cross_kind_safeguards() {
    let docs = vec![
        document(
            1,
            ThreadKind::Issue,
            "Cache eviction memory crash",
            None,
            &[1.0, 0.0],
        ),
        document(
            2,
            ThreadKind::Issue,
            "Memory crash after eviction",
            None,
            &[0.85, 0.5267827],
        ),
        document(
            3,
            ThreadKind::Issue,
            "Unrelated clipboard outage",
            None,
            &[0.0, 1.0],
        ),
        document(
            4,
            ThreadKind::PullRequest,
            "Cache eviction memory crash",
            None,
            &[0.92, -0.39191836],
        ),
        document(
            5,
            ThreadKind::PullRequest,
            "Cache eviction memory crash",
            None,
            &[0.95, 0.3122499],
        ),
    ];
    let (clusters, edge_count) = build_cluster_candidates(
        docs,
        "example/repo",
        ClusterOptions {
            threshold: 0.80,
            cross_kind_threshold: 0.93,
            fanout: 16,
            max_cluster_size: 40,
            min_cluster_size: 1,
        },
        &CancellationToken::new(),
    )
    .expect("build graph");
    assert_eq!(edge_count, 3);
    assert_eq!(
        clusters
            .iter()
            .map(|cluster| cluster.members.len())
            .collect::<Vec<_>>(),
        [3, 1, 1]
    );
    assert_eq!(clusters[0].representative.number().get(), 1);
}

#[test]
fn references_are_repository_scoped_and_early_body_evidence_is_strong() {
    let docs = vec![
        document(
            101,
            ThreadKind::Issue,
            "Token expires too early",
            None,
            &[1.0, 0.0],
        ),
        document(
            102,
            ThreadKind::PullRequest,
            "Unrelated patch",
            Some("See #101 for the report"),
            &[0.0, 1.0],
        ),
        document(
            103,
            ThreadKind::Issue,
            "Different repository",
            Some("example/other#101"),
            &[0.0, -1.0],
        ),
    ];
    let (clusters, edge_count) = build_cluster_candidates(
        docs,
        "example/repo",
        ClusterOptions::default(),
        &CancellationToken::new(),
    )
    .expect("build graph");
    assert_eq!(edge_count, 1, "clusters: {clusters:#?}");
    let referenced = clusters
        .iter()
        .find(|cluster| cluster.members.len() == 2)
        .expect("reference cluster");
    assert_eq!(referenced.members[0].score_to_representative, Some(1.0));
}

#[test]
fn fanout_and_maximum_size_keep_deterministic_components() {
    let docs = (1..=6)
        .map(|number| {
            document(
                number,
                ThreadKind::Issue,
                "Shared memory issue",
                None,
                &[1.0, number as f32 / 100.0],
            )
        })
        .collect::<Vec<_>>();
    let (first, _) = build_cluster_candidates(
        docs.clone(),
        "example/repo",
        ClusterOptions {
            fanout: 1,
            max_cluster_size: 3,
            ..ClusterOptions::default()
        },
        &CancellationToken::new(),
    )
    .expect("first graph");
    let (second, _) = build_cluster_candidates(
        docs,
        "example/repo",
        ClusterOptions {
            fanout: 1,
            max_cluster_size: 3,
            ..ClusterOptions::default()
        },
        &CancellationToken::new(),
    )
    .expect("second graph");
    let memberships = |clusters: &[ClusterCandidate]| {
        clusters
            .iter()
            .map(|cluster| {
                cluster
                    .members
                    .iter()
                    .map(|member| member.summary.discussion.id.number().get())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(memberships(&first), memberships(&second));
    assert!(first.iter().all(|cluster| cluster.members.len() <= 3));
}

#[test]
fn cancellation_stops_graph_construction() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let error = build_cluster_candidates(
        vec![document(1, ThreadKind::Issue, "One", None, &[1.0, 0.0])],
        "example/repo",
        ClusterOptions::default(),
        &cancellation,
    )
    .expect_err("cancelled graph");
    assert_eq!(error.code(), "operation_cancelled");
}

fn document(
    number: u64,
    kind: ThreadKind,
    title: &str,
    body: Option<&str>,
    vector_values: &[f32],
) -> EmbeddingSearchDocument {
    let host = GitHubHost::parse("github.com").expect("host");
    let repository_id = RepositoryId::new(
        host,
        ProviderId::new("repo-1").expect("repository provider ID"),
    );
    let identity = ThreadId::new(
        repository_id.clone(),
        ProviderId::new(format!("thread-{number}")).expect("thread provider ID"),
        ThreadNumber::new(number).expect("thread number"),
    );
    let timestamp = UtcTimestamp::parse("2026-09-28T00:00:00Z").expect("timestamp");
    let summary = ThreadSummary {
        repository: Repository {
            id: repository_id,
            owner: "example".to_owned(),
            name: "repo".to_owned(),
            full_name: "example/repo".to_owned(),
            default_branch: None,
            updated_at: Some(timestamp),
            provider_data: ProviderData::default(),
        },
        discussion: Discussion {
            id: identity,
            kind,
            state: SourceState::Open,
            title: title.to_owned(),
            body: body.map(str::to_owned),
            html_url: None,
            created_at: timestamp,
            updated_at: timestamp,
            closed_at: None,
            labels: Vec::new(),
            assignees: Vec::new(),
            provider_data: ProviderData::default(),
        },
        coverage: Vec::<Coverage>::new(),
    };
    let vector = EmbeddingVector::new(vector_values.to_vec(), None).expect("vector");
    EmbeddingSearchDocument {
        summary,
        chunks: vec![StoredEmbeddingChunk {
            index: 0,
            count: 1,
            chunk_hash: format!("{:064x}", number),
            vector,
        }],
    }
}
