//! Comparable-vector cosine arithmetic.
//!
//! Equal dimensions alone do not establish that two vectors came from the same embedding space;
//! callers own model and recipe compatibility.

use forgesync_core::embedding::EmbeddingVector;

/// Computes cosine similarity, clamped to -1 through 1.
///
/// Accumulates in f64, which cannot overflow for finite f32 components. Returns `None` for
/// differing dimensions or a zero-magnitude vector.
pub fn cosine_similarity(left: &EmbeddingVector, right: &EmbeddingVector) -> Option<f64> {
    if left.dimensions() != right.dimensions() {
        return None;
    }
    let mut dot = 0.0_f64;
    let mut left_norm = 0.0_f64;
    let mut right_norm = 0.0_f64;
    for (&left, &right) in left.values().iter().zip(right.values()) {
        let (left, right) = (f64::from(left), f64::from(right));
        dot += left * right;
        left_norm += left * left;
        right_norm += right * right;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        return None;
    }
    Some((dot / (left_norm.sqrt() * right_norm.sqrt())).clamp(-1.0, 1.0))
}

#[cfg(test)]
#[path = "exact_search/tests.rs"]
mod tests;
