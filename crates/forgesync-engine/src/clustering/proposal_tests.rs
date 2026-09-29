//! # Representative and member-score projection scenarios
//!
//! These cases start from explicit retained edges rather than asking vector selection to create a
//! particular graph. That keeps the projection contract visible: degree chooses representatives,
//! identity breaks ties, and transitive-only membership has no invented direct score.
//!
//! The shared document fixture supplies static domain records without acquisition or assertions.
//! Each test calls the projection once and names the expected representative or member weight.
//! Graph eligibility and bounded union behavior are covered in the neighboring candidate suite.
//! Store reconciliation of local decisions remains an integration concern outside this module.

use forgesync_core::content::ThreadKind;

use crate::clustering::evidence::CandidateEdge;
use crate::clustering::proposals::format_clusters;
use crate::clustering::test_documents::document;

#[test]
fn highest_retained_degree_selects_the_representative() {
    let documents = vec![
        document(1, ThreadKind::Issue, "First", None, &[1.0, 0.0]),
        document(2, ThreadKind::Issue, "Center", None, &[1.0, 0.0]),
        document(3, ThreadKind::Issue, "Third", None, &[1.0, 0.0]),
    ];
    let edges = [
        CandidateEdge {
            left: 0,
            right: 1,
            score: 0.9,
        },
        CandidateEdge {
            left: 1,
            right: 2,
            score: 0.85,
        },
    ];

    let proposals = format_clusters(&documents, &[vec![0, 1, 2]], &edges, 1);

    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].representative.number().get(), 2);
    assert_eq!(proposals[0].title, "Center");
    assert_eq!(proposals[0].members[0].score_to_representative, Some(0.9));
    assert_eq!(proposals[0].members[1].score_to_representative, Some(1.0));
    assert_eq!(proposals[0].members[2].score_to_representative, Some(0.85));
}

#[test]
fn equal_degrees_choose_the_lower_discussion_number() {
    let documents = vec![
        document(5, ThreadKind::Issue, "Earlier", None, &[1.0, 0.0]),
        document(10, ThreadKind::Issue, "Later", None, &[1.0, 0.0]),
    ];
    let edges = [CandidateEdge {
        left: 0,
        right: 1,
        score: 0.94,
    }];

    let proposals = format_clusters(&documents, &[vec![0, 1]], &edges, 1);

    assert_eq!(proposals[0].representative.number().get(), 5);
    assert_eq!(proposals[0].members[1].score_to_representative, Some(0.94));
}

#[test]
fn transitive_membership_does_not_invent_a_direct_representative_score() {
    let documents = vec![
        document(1, ThreadKind::Issue, "First", None, &[1.0, 0.0]),
        document(2, ThreadKind::Issue, "Second", None, &[1.0, 0.0]),
        document(3, ThreadKind::Issue, "Third", None, &[1.0, 0.0]),
        document(4, ThreadKind::Issue, "Fourth", None, &[1.0, 0.0]),
    ];
    let edges = [
        CandidateEdge {
            left: 0,
            right: 1,
            score: 0.9,
        },
        CandidateEdge {
            left: 1,
            right: 2,
            score: 0.85,
        },
        CandidateEdge {
            left: 2,
            right: 3,
            score: 0.8,
        },
    ];

    let proposals = format_clusters(&documents, &[vec![0, 1, 2, 3]], &edges, 1);

    assert_eq!(proposals[0].representative.number().get(), 2);
    assert_eq!(
        proposals[0].members[3].summary.discussion.id.number().get(),
        4
    );
    assert_eq!(proposals[0].members[3].score_to_representative, None);
}

#[test]
fn singleton_representative_has_a_self_score() {
    let documents = vec![document(1, ThreadKind::Issue, "Only", None, &[1.0, 0.0])];

    let proposals = format_clusters(&documents, &[vec![0]], &[], 1);

    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].members[0].score_to_representative, Some(1.0));
}

#[test]
fn components_smaller_than_the_selected_minimum_are_omitted() {
    let documents = vec![document(1, ThreadKind::Issue, "Only", None, &[1.0, 0.0])];

    let proposals = format_clusters(&documents, &[vec![0]], &[], 2);

    assert!(proposals.is_empty());
}
