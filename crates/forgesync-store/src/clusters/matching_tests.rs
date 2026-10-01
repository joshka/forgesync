//! Durable identity assignment from explicit membership.

use std::collections::HashMap;

use crate::clusters::generation::PreparedCluster;
use crate::clusters::matching::{ExistingCluster, match_cluster_identities};

#[test]
fn strongest_absolute_overlap_claims_the_durable_identity() {
    let existing = [previous(10, &[1, 2, 3])];
    let generated = [proposed(&[1]), proposed(&[2, 3, 4, 5, 6, 7, 8, 9])];

    let matches = match_cluster_identities(&existing, &generated);

    assert_eq!(matches, HashMap::from([(1, 10)]));
}

#[test]
fn proportional_overlap_breaks_equal_absolute_overlap() {
    let existing = [previous(10, &[1, 2, 3, 4]), previous(20, &[1, 2])];
    let generated = [proposed(&[1, 2])];

    let matches = match_cluster_identities(&existing, &generated);

    assert_eq!(matches, HashMap::from([(0, 20)]));
}

#[test]
fn smaller_durable_identity_wins_equal_membership_evidence() {
    let existing = [previous(20, &[1, 2]), previous(10, &[1, 2])];
    let generated = [proposed(&[1, 2])];

    let matches = match_cluster_identities(&existing, &generated);

    assert_eq!(matches, HashMap::from([(0, 10)]));
}

#[test]
fn earlier_generated_position_wins_when_durable_identity_and_evidence_tie() {
    let existing = [previous(10, &[1, 2])];
    let generated = [proposed(&[1]), proposed(&[2])];

    let matches = match_cluster_identities(&existing, &generated);

    assert_eq!(matches, HashMap::from([(0, 10)]));
}

#[test]
fn one_to_one_assignment_uses_remaining_evidence_after_the_strongest_match() {
    let existing = [previous(10, &[1, 2]), previous(20, &[2, 3])];
    let generated = [proposed(&[1, 2]), proposed(&[3])];

    let matches = match_cluster_identities(&existing, &generated);

    assert_eq!(matches, HashMap::from([(0, 10), (1, 20)]));
}

#[test]
fn disjoint_membership_leaves_the_proposal_without_a_reused_identity() {
    let existing = [previous(10, &[1, 2])];
    let generated = [proposed(&[3])];

    let matches = match_cluster_identities(&existing, &generated);

    assert_eq!(matches, HashMap::new());
}

/// Constructs durable membership evidence in the same set representation as archive reads.
fn previous(id: i64, members: &[i64]) -> ExistingCluster {
    ExistingCluster {
        id,
        members: members.iter().copied().collect(),
    }
}

/// Constructs a nonempty SQL-resolved proposal; title and scores do not affect identity matching.
fn proposed(members: &[i64]) -> PreparedCluster {
    PreparedCluster {
        representative_id: members[0],
        title: "Fixture cluster".to_owned(),
        members: members.iter().map(|id| (*id, None)).collect(),
    }
}
