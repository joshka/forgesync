//! # Retain durable cluster identity across generations
//!
//! Existing active and excluded membership is loaded in durable row order, then compared with
//! proposed membership. Removed members do not contribute evidence. Matching greedily assigns the
//! strongest overlap first, using proportional overlap and stable row/index tie breaks.
//!
//! Each existing and generated group can participate in at most one assignment. The returned map
//! relates generated positions to durable row IDs; unmatched proposals receive new membership keys
//! during generation writes. Local decisions therefore remain associated with a reused identity.
//! These operations read and calculate within the caller's transaction; they neither write nor
//! commit. Prepared membership comes from `generation_input` after identity validation.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use sqlx::{Row, SqliteConnection};

use crate::clusters::generation_input::PreparedCluster;
use crate::error::StoreError;

/// Durable group with active or excluded membership used for identity overlap.
#[derive(Debug)]
pub struct ExistingCluster {
    /// Existing durable cluster row identity.
    pub id: i64,
    /// Membership evidence; removed rows do not participate in matching.
    pub members: HashSet<i64>,
}

/// Loads durable row identities and active/excluded membership for overlap matching.
pub async fn load_existing_clusters(
    connection: &mut SqliteConnection,
    repository_id: i64,
) -> Result<Vec<ExistingCluster>, StoreError> {
    let rows = sqlx::query(
        "SELECT c.id, cm.thread_id FROM clusters c LEFT JOIN cluster_memberships cm ON cm.cluster_id = c.id AND cm.state IN ('active', 'excluded') WHERE c.repository_id = ? ORDER BY c.id, cm.thread_id",
    )
    .bind(repository_id)
    .fetch_all(&mut *connection)
    .await?;
    let mut clusters = Vec::<ExistingCluster>::new();
    for row in rows {
        let id: i64 = row.try_get("id")?;
        if clusters.last().is_none_or(|cluster| cluster.id != id) {
            clusters.push(ExistingCluster {
                id,
                members: HashSet::new(),
            });
        }
        let thread_id: Option<i64> = row.try_get("thread_id")?;
        if let Some(thread_id) = thread_id {
            clusters
                .last_mut()
                .expect("cluster row was inserted")
                .members
                .insert(thread_id);
        }
    }
    Ok(clusters)
}

/// Reuses durable cluster IDs by assigning the strongest membership overlaps first. The ordered
/// tie breaks keep equal evidence from producing different IDs on repeated builds.
pub fn match_cluster_identities(
    existing: &[ExistingCluster],
    generated: &[PreparedCluster],
) -> HashMap<usize, i64> {
    let mut candidates = matching_candidates(existing, generated);
    candidates.sort_by(MembershipOverlap::priority_cmp);
    assign_candidates(candidates)
}

/// Enumerates only group pairs with shared membership, retaining stable generated positions.
fn matching_candidates(
    existing: &[ExistingCluster],
    generated: &[PreparedCluster],
) -> Vec<MembershipOverlap> {
    let mut candidates = Vec::new();
    for (generated_index, current) in generated.iter().enumerate() {
        let current_members = current
            .members
            .iter()
            .map(|(thread_id, _)| *thread_id)
            .collect::<HashSet<_>>();
        for previous in existing {
            if let Some(candidate) =
                MembershipOverlap::between(&current_members, previous, generated_index)
            {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

/// Greedily assigns strongest evidence without reusing either side of a match.
fn assign_candidates(candidates: Vec<MembershipOverlap>) -> HashMap<usize, i64> {
    let mut used_generated = HashSet::new();
    let mut used_existing = HashSet::new();
    let mut matches = HashMap::new();
    for candidate in candidates {
        if !used_generated.contains(&candidate.generated_index)
            && !used_existing.contains(&candidate.existing_id)
        {
            used_generated.insert(candidate.generated_index);
            used_existing.insert(candidate.existing_id);
            matches.insert(candidate.generated_index, candidate.existing_id);
        }
    }
    matches
}

/// Positive membership overlap between one generated position and one durable group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MembershipOverlap {
    /// Shared discussion rows; absolute overlap has highest ranking priority.
    overlap: usize,
    /// Combined distinct membership, used for the proportional-overlap tie break.
    union: usize,
    /// Position in the prepared generation, used as the final deterministic tie break.
    generated_index: usize,
    /// Durable cluster row identity, preferred in ascending order when evidence ties.
    existing_id: i64,
}

impl MembershipOverlap {
    /// Builds evidence only when the generated and existing groups share at least one member.
    fn between(
        current: &HashSet<i64>,
        previous: &ExistingCluster,
        generated_index: usize,
    ) -> Option<Self> {
        let overlap = current.intersection(&previous.members).count();
        if overlap == 0 {
            return None;
        }
        let union = current.len() + previous.members.len() - overlap;
        Some(Self {
            overlap,
            union,
            generated_index,
            existing_id: previous.id,
        })
    }

    /// Orders strongest evidence first: absolute overlap, proportional overlap, durable ID,
    /// position. Cross multiplication compares fractions without floating-point rounding.
    /// Products fit in `u128` on supported targets, whose collection lengths are at most 64
    /// bits.
    fn priority_cmp(&self, other: &Self) -> Ordering {
        other
            .overlap
            .cmp(&self.overlap)
            .then_with(|| self.proportional_cmp(other))
            .then_with(|| self.existing_id.cmp(&other.existing_id))
            .then_with(|| self.generated_index.cmp(&other.generated_index))
    }

    /// Compares proportional overlap in descending order without dividing the membership counts.
    fn proportional_cmp(&self, other: &Self) -> Ordering {
        let other_weight = u128::try_from(other.overlap).unwrap_or(u128::MAX)
            * u128::try_from(self.union).unwrap_or(u128::MAX);
        let self_weight = u128::try_from(self.overlap).unwrap_or(u128::MAX)
            * u128::try_from(other.union).unwrap_or(u128::MAX);
        other_weight.cmp(&self_weight)
    }
}

#[cfg(test)]
#[path = "generation_matching_tests.rs"]
mod tests;
