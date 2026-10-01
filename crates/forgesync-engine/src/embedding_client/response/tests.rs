//! Response validation scenarios decoded through the real wire shape.

use serde_json::json;

use crate::embedding_client::EmbeddingClientError;
use crate::embedding_client::response::EmbeddingResponse;

#[rstest::rstest]
#[case::missing_item(json!({"data": [{"index": 0, "embedding": [1.0, 0.0]}]}), EmbeddingClientError::InvalidResponse)]
#[case::duplicate_index(json!({"data": [
    {"index": 0, "embedding": [1.0, 0.0]},
    {"index": 0, "embedding": [0.0, 1.0]}
]}), EmbeddingClientError::InvalidResponse)]
#[case::out_of_range_index(json!({"data": [
    {"index": 0, "embedding": [1.0, 0.0]},
    {"index": 2, "embedding": [0.0, 1.0]}
]}), EmbeddingClientError::InvalidResponse)]
#[case::zero_vector(json!({"data": [
    {"index": 0, "embedding": [0.0, 0.0]},
    {"index": 1, "embedding": [0.0, 1.0]}
]}), EmbeddingClientError::InvalidVector)]
#[case::wrong_dimensions(json!({"data": [
    {"index": 0, "embedding": [1.0]},
    {"index": 1, "embedding": [0.0, 1.0]}
]}), EmbeddingClientError::InvalidVector)]
#[case::out_of_range_value(json!({"data": [
    {"index": 0, "embedding": [3.5e39, 0.0]},
    {"index": 1, "embedding": [0.0, 1.0]}
]}), EmbeddingClientError::InvalidVector)]
fn malformed_indices_and_vectors_are_rejected(
    #[case] response: serde_json::Value,
    #[case] expected: EmbeddingClientError,
) {
    let response = serde_json::from_value::<EmbeddingResponse>(response).expect("typed fixture");

    let result = response.validate(2, Some(2));

    assert_eq!(result, Err(expected));
}

#[test]
fn differing_model_echo_is_accepted() {
    let response = serde_json::from_value::<EmbeddingResponse>(json!({
        "model": "fixture-model-2026-09-01",
        "data": [
            {"index": 1, "embedding": [0.0, 1.0]},
            {"index": 0, "embedding": [1.0, 0.0]}
        ]
    }))
    .expect("typed fixture");

    let vectors = response.validate(2, Some(2)).expect("aliased model echo");

    assert_eq!(vectors[0].values(), [1.0, 0.0]);
    assert_eq!(vectors[1].values(), [0.0, 1.0]);
}
