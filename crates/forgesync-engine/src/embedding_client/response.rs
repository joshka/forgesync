//! Validate embedding responses before persistence.
//!
//! Items may arrive reordered; duplicate or missing indexes reject the whole batch. A model echo is
//! ignored: services commonly echo a versioned or aliased name for the requested model.

use forgesync_core::embedding::EmbeddingVector;
use serde::Deserialize;

use crate::embedding_client::EmbeddingClientError;

/// Maximum decoded vector width accepted before numeric conversion.
const MAX_EMBEDDING_DIMENSIONS: usize = 65_536;

/// Raw service envelope awaiting request-relative validation.
#[derive(Deserialize)]
pub struct EmbeddingResponse {
    /// Indexed vector items, which may arrive in a different order than the request.
    data: Vec<EmbeddingResponseItem>,
}

/// One unvalidated vector and its zero-based input position.
#[derive(Deserialize)]
struct EmbeddingResponseItem {
    /// Provider-declared input position; duplicates and out-of-range values are rejected.
    index: usize,
    /// JSON numbers converted to checked finite, nonzero domain vectors.
    embedding: Vec<f64>,
}

impl EmbeddingResponse {
    /// Checks this response against the request and returns vectors in original input order.
    ///
    /// Accepts reordered items but requires exactly one index for every input. Configured
    /// dimensions constrain every vector; otherwise the first decoded vector establishes the
    /// batch dimension. Numeric conversion can lose f64 precision, while core validation
    /// rejects nonfinite/zero or incompatible f32 results. No partial vector list escapes.
    ///
    /// # Errors
    ///
    /// Count/index failures return `InvalidResponse`. Width, numeric, or dimension failures
    /// return `InvalidVector`.
    pub fn validate(
        self,
        input_count: usize,
        expected_dimensions: Option<u32>,
    ) -> Result<Vec<EmbeddingVector>, EmbeddingClientError> {
        if self.data.len() != input_count {
            return Err(EmbeddingClientError::InvalidResponse);
        }
        let mut indexed: Vec<Option<EmbeddingVector>> =
            std::iter::repeat_with(|| None).take(input_count).collect();
        let mut dimensions = expected_dimensions;
        for item in self.data {
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
}

#[cfg(test)]
mod tests;
