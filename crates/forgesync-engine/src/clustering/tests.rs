//! Cluster candidate policy.

use forgesync_core::content::ThreadKind;
use tokio_util::sync::CancellationToken;

use crate::clustering::ClusterOptions;
use crate::clustering::candidates::build_cluster_candidates;
use crate::clustering::test_documents::document;

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
