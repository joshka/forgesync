//! # Validate embedding responses before persistence
//!
//! `EmbeddingResponse` is the accepted shape of one service response.
//! [`EmbeddingResponse::validate`] checks its relation to the submitted input, including vector
//! count and usable numeric contents.
//!
//! Only validated vectors should reach `embeddings` or the store. Keeping response checks here
//! prevents a transport success status from being mistaken for a usable search representation.
//!
//! The raw envelope and indexed items are private implementation shapes. The consuming validation
//! method relates them to the submitted count, expected model, and optional configured dimensions.
//! It accepts provider item reordering and reconstructs request order; duplicate/missing indexes
//! reject the entire batch. Model omission is allowed, while a conflicting echo is not.
//!
//! Each numeric vector is converted to f32 and checked by the core value type. Dimension agreement
//! is enforced across the batch even when the request did not configure a dimension. Transport
//! owns body bounds and status checks; this leaf owns decoded response meaning, with no archive
//! I/O.

use forgesync_core::embedding::EmbeddingVector;
use serde::Deserialize;

use crate::embedding_client::EmbeddingClientError;

/// Maximum decoded vector width accepted before numeric conversion.
const MAX_EMBEDDING_DIMENSIONS: usize = 65_536;

/// Raw service envelope awaiting request-relative validation.
///
/// Deserialization establishes JSON shape only. The model can be absent; when supplied its spelling
/// must match the requested model. No raw envelope is persisted or returned as a valid vector
/// batch.
#[derive(Deserialize)]
pub struct EmbeddingResponse {
    /// Indexed vector items, which may arrive in a different order than the request.
    data: Vec<EmbeddingResponseItem>,
    #[serde(default)]
    /// Optional exact model echo, not a discovered or substituted model choice.
    model: Option<String>,
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
    /// Accepts reordered items but requires exactly one index for every input. A present model echo
    /// must match exactly. Configured dimensions constrain every vector; otherwise the first
    /// decoded vector establishes the batch dimension. Numeric conversion can lose f64
    /// precision, while core validation rejects nonfinite/zero or incompatible f32 results. No
    /// partial vector list escapes.
    ///
    /// # Errors
    ///
    /// Model/count/index failures return `InvalidResponse`. Width, numeric, or dimension failures
    /// return `InvalidVector`; this method does not retry, call the service, or persist data.
    pub fn validate(
        self,
        input_count: usize,
        expected_dimensions: Option<u32>,
        expected_model: &str,
    ) -> Result<Vec<EmbeddingVector>, EmbeddingClientError> {
        if self
            .model
            .as_deref()
            .is_some_and(|model| model != expected_model)
        {
            return Err(EmbeddingClientError::InvalidResponse);
        }
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
