//! Strict embedding response decoding and vector validation.

use forgesync_core::embedding::EmbeddingVector;
use serde::Deserialize;

use super::EmbeddingClientError;

const MAX_EMBEDDING_DIMENSIONS: usize = 65_536;

#[derive(Deserialize)]
pub struct EmbeddingResponse {
    data: Vec<EmbeddingResponseItem>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponseItem {
    index: usize,
    embedding: Vec<f64>,
}

pub fn validate_response(
    response: EmbeddingResponse,
    input_count: usize,
    expected_dimensions: Option<u32>,
    expected_model: &str,
) -> Result<Vec<EmbeddingVector>, EmbeddingClientError> {
    if response
        .model
        .as_deref()
        .is_some_and(|model| model != expected_model)
    {
        return Err(EmbeddingClientError::InvalidResponse);
    }
    if response.data.len() != input_count {
        return Err(EmbeddingClientError::InvalidResponse);
    }
    let mut indexed: Vec<Option<EmbeddingVector>> =
        std::iter::repeat_with(|| None).take(input_count).collect();
    let mut dimensions = expected_dimensions;
    for item in response.data {
        if item.index >= input_count || indexed[item.index].is_some() {
            return Err(EmbeddingClientError::InvalidResponse);
        }
        if item.embedding.len() > MAX_EMBEDDING_DIMENSIONS {
            return Err(EmbeddingClientError::InvalidVector);
        }
        let values = item
            .embedding
            .into_iter()
            .map(|value| value as f32)
            .collect::<Vec<_>>();
        let vector = EmbeddingVector::new(values, dimensions)?;
        dimensions = Some(vector.dimensions());
        indexed[item.index] = Some(vector);
    }
    indexed
        .into_iter()
        .map(|vector| vector.ok_or(EmbeddingClientError::InvalidResponse))
        .collect()
}
