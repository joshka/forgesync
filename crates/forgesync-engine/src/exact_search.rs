//! # Comparable-vector cosine arithmetic
//!
//! [`cosine_similarity`] compares validated embedding vectors using scaled f64 accumulation,
//! avoiding overflow from large finite f32 components. Equal dimensions and nonzero magnitude
//! are required; the result is clamped to the inclusive range -1 through 1.
//!
//! This operation has no archive or service dependency. Vector constructors own finite-value
//! validation, while callers own model and recipe compatibility. Equal dimensions alone do not
//! establish that two vectors came from the same embedding space.
//!
//! Internal ranked-candidate policy lives in the private scoring module: document chunk maxima,
//! positive-score filtering, ordering, and bounded page merging. This public helper supplies only
//! the arithmetic needed by semantic retrieval and clustering evidence.

use forgesync_core::embedding::EmbeddingVector;

/// Computes cosine similarity using scaled f64 accumulation to avoid f32 overflow.
///
/// Returns `None` for differing dimensions or zero magnitude. Otherwise returns a score clamped
/// to the inclusive range -1 through 1. Vector construction owns finite-value validation; this
/// calculation does not compare model identity or establish that the vectors share a model.
pub fn cosine_similarity(left: &EmbeddingVector, right: &EmbeddingVector) -> Option<f64> {
    if left.dimensions() != right.dimensions() {
        return None;
    }
    let left_values = left.values();
    let right_values = right.values();
    let mut left_scale = 0.0_f64;
    let mut right_scale = 0.0_f64;
    for (&left_value, &right_value) in left_values.iter().zip(right_values) {
        left_scale = left_scale.max(f64::from(left_value).abs());
        right_scale = right_scale.max(f64::from(right_value).abs());
    }
    if left_scale == 0.0 || right_scale == 0.0 {
        return None;
    }

    let mut dot = 0.0_f64;
    let mut left_norm = 0.0_f64;
    let mut right_norm = 0.0_f64;
    for (&left_value, &right_value) in left_values.iter().zip(right_values) {
        let left_value = f64::from(left_value) / left_scale;
        let right_value = f64::from(right_value) / right_scale;
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }
    let score = dot / (left_norm.sqrt() * right_norm.sqrt());
    Some(score.clamp(-1.0, 1.0))
}

#[cfg(test)]
#[path = "exact_search/tests.rs"]
mod tests;
