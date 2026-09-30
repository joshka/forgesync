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
    let mut candidates = Vec::<(usize, usize, usize, i64)>::new();
    for (generated_index, current) in generated.iter().enumerate() {
        let current_members = current
            .members
            .iter()
            .map(|(thread_id, _)| *thread_id)
            .collect::<HashSet<_>>();
        for previous in existing {
            let overlap = current_members.intersection(&previous.members).count();
            if overlap > 0 {
                let union = current_members.len() + previous.members.len() - overlap;
                candidates.push((overlap, union, generated_index, previous.id));
            }
        }
    }
    candidates.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| {
                (u128::try_from(right.0).unwrap_or(u128::MAX)
                    * u128::try_from(left.1).unwrap_or(u128::MAX))
                .cmp(
                    &(u128::try_from(left.0).unwrap_or(u128::MAX)
                        * u128::try_from(right.1).unwrap_or(u128::MAX)),
                )
            })
            .then_with(|| left.3.cmp(&right.3))
            .then_with(|| left.2.cmp(&right.2))
    });
    let mut used_generated = HashSet::new();
    let mut used_existing = HashSet::new();
    let mut matches = HashMap::new();
    for (_, _, generated_index, existing_id) in candidates {
        if !used_generated.contains(&generated_index) && !used_existing.contains(&existing_id) {
            used_generated.insert(generated_index);
            used_existing.insert(existing_id);
            matches.insert(generated_index, existing_id);
        }
    }
    matches
}
