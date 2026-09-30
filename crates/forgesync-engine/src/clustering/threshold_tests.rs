//! # Pairwise title and discussion-kind safeguards
//!
//! Two-document cases isolate title support from the separately configured cross-kind threshold.
//! Moderate same-kind similarity needs shared title tokens; strong similarity can stand alone.
//! Cross-kind cases use identical titles so only vector strength changes their acceptance.
//! Minimum component size is two, making rejected edges produce no reported proposal.
//!
//! Every case constructs supplied vectors directly and invokes the real graph coordinator.
//! The shared fixture performs no graph selection, model requests, or archive writes.
//! Neighbor bounds, reference scoping, and cancellation have separate graph tests.
//! Projection tests own representative selection and per-member evidence scores.

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
fn high_confidence_same_kind_similarity_needs_no_title_overlap() {
    let docs = vec![
        document(1, ThreadKind::Issue, "Memory crash", None, &[1.0, 0.0]),
        document(
            2,
            ThreadKind::Issue,
            "Clipboard outage",
            None,
            &[0.95, 0.3122499],
        ),
    ];
    let options = ClusterOptions {
        min_cluster_size: 2,
        ..ClusterOptions::default()
    };

    let (clusters, edges) =
        build_cluster_candidates(docs, "example/repo", options, &CancellationToken::new())
            .expect("build high-confidence graph");

    assert_eq!(edges, 1);
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].members.len(), 2);
}
