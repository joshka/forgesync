//! # Cluster candidate policy
//!
//! These tests document safeguards in the candidate graph: weak title evidence, cross-kind
//! relationships, repository-scoped references, fanout, and maximum component size. The exact
//! fixture relationships matter because transitive grouping can make a plausible pair produce an
//! implausible cluster. Read these alongside `candidates` before changing thresholds or union
//! behavior; a new candidate rule should have a small example that explains the intended grouping.
//!
//! Documents are construction-only projections with supplied vectors and body references.
//! Graph operations run directly; fixtures do not select expected edges or persist generations.
//! Member identities accompany size expectations so unrelated discussions cannot substitute for
//! intended neighbors. Repeated construction checks determinism of the same bounded policy.
//! Writer fencing and local decision reconciliation are separate workflow/store contracts.

use forgesync_core::content::ThreadKind;
use tokio_util::sync::CancellationToken;

use crate::clustering::ClusterOptions;
use crate::clustering::candidates::build_cluster_candidates;
use crate::clustering::test_documents::document;

#[rstest::rstest]
#[case::shared_title("Memory crash after eviction", 1)]
#[case::unrelated_title("Unrelated clipboard outage", 0)]
fn moderate_similarity_requires_title_support(#[case] title: &str, #[case] expected_edges: usize) {
    let docs = vec![
        document(
            1,
            ThreadKind::Issue,
            "Cache eviction memory crash",
            None,
            &[1.0, 0.0],
        ),
        document(2, ThreadKind::Issue, title, None, &[0.85, 0.5267827]),
    ];
    let options = ClusterOptions {
        threshold: 0.80,
        min_cluster_size: 2,
        ..ClusterOptions::default()
    };

    let (clusters, edges) =
        build_cluster_candidates(docs, "example/repo", options, &CancellationToken::new())
            .expect("build title policy graph");

    assert_eq!(edges, expected_edges);
    assert_eq!(clusters.len(), expected_edges);
}

#[rstest::rstest]
#[case::below_cross_kind_threshold(&[0.92, -0.39191836], 0)]
#[case::above_cross_kind_threshold(&[0.95, 0.3122499], 1)]
fn cross_kind_similarity_requires_its_own_threshold(
    #[case] vector: &[f32],
    #[case] expected_edges: usize,
) {
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
            ThreadKind::PullRequest,
            "Cache eviction memory crash",
            None,
            vector,
        ),
    ];
    let options = ClusterOptions {
        threshold: 0.80,
        cross_kind_threshold: 0.93,
        min_cluster_size: 2,
        ..ClusterOptions::default()
    };

    let (clusters, edges) =
        build_cluster_candidates(docs, "example/repo", options, &CancellationToken::new())
            .expect("build kind policy graph");

    assert_eq!(edges, expected_edges);
    assert_eq!(clusters.len(), expected_edges);
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
    let members = referenced
        .members
        .iter()
        .map(|member| member.summary.discussion.id.number().get())
        .collect::<Vec<_>>();
    assert_eq!(members, [101, 102]);
}

#[test]
fn fanout_and_maximum_size_keep_deterministic_components() {
    let docs = vec![
        document(
            1,
            ThreadKind::Issue,
            "Shared memory issue",
            None,
            &[1.0, 0.01],
        ),
        document(
            2,
            ThreadKind::Issue,
            "Shared memory issue",
            None,
            &[1.0, 0.02],
        ),
        document(
            3,
            ThreadKind::Issue,
            "Shared memory issue",
            None,
            &[1.0, 0.03],
        ),
        document(
            4,
            ThreadKind::Issue,
            "Shared memory issue",
            None,
            &[1.0, 0.04],
        ),
        document(
            5,
            ThreadKind::Issue,
            "Shared memory issue",
            None,
            &[1.0, 0.05],
        ),
        document(
            6,
            ThreadKind::Issue,
            "Shared memory issue",
            None,
            &[1.0, 0.06],
        ),
    ];
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
    assert_eq!(first, second);
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].members.len(), 3);
    assert_eq!(first[1].members.len(), 3);
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
