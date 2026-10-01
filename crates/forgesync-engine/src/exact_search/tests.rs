//! Cosine arithmetic for axis, diagonal, mismatched, and large finite components.

use forgesync_core::embedding::EmbeddingVector;

use crate::exact_search::cosine_similarity;

#[rstest::rstest]
#[case::parallel(&[1.0, 0.0], 1.0)]
#[case::orthogonal(&[0.0, 1.0], 0.0)]
#[case::opposite(&[-1.0, 0.0], -1.0)]
fn cosine_similarity_matches_axis_directions(#[case] values: &[f32], #[case] expected: f64) {
    let left = EmbeddingVector::new(vec![1.0, 0.0], None).expect("valid left vector");
    let right = EmbeddingVector::new(values.to_vec(), None).expect("valid right vector");

    assert_eq!(cosine_similarity(&left, &right), Some(expected));
}

#[test]
fn cosine_similarity_matches_diagonal_direction() {
    let left = EmbeddingVector::new(vec![1.0, 0.0], None).expect("valid left vector");
    let right = EmbeddingVector::new(vec![0.5, 0.5], None).expect("valid right vector");

    let score = cosine_similarity(&left, &right).expect("comparable vectors");
    let difference = (score - std::f64::consts::FRAC_1_SQRT_2).abs();
    assert!(difference < 1e-7, "diagonal score difference: {difference}");
}

#[test]
fn cosine_similarity_rejects_dimension_mismatch() {
    let left = EmbeddingVector::new(vec![1.0, 0.0], None).expect("valid left vector");
    let right = EmbeddingVector::new(vec![1.0], None).expect("valid right vector");

    assert_eq!(cosine_similarity(&left, &right), None);
}

#[test]
fn cosine_similarity_handles_large_finite_components() {
    let left = EmbeddingVector::new(vec![f32::MAX, f32::MAX], None).expect("finite vector");
    let right = EmbeddingVector::new(vec![f32::MAX, f32::MAX], None).expect("finite vector");

    let score = cosine_similarity(&left, &right).expect("comparable vectors");
    assert!((score - 1.0).abs() < 1e-12, "parallel score: {score}");
}
