//! Validated vectors at the boundary of semantic retrieval.
//!
//! The vector does not carry a model name or document recipe; the engine and store pair it with
//! that metadata so vectors from incompatible spaces are not mixed.
//!
//! ```
//! use forgesync_core::embedding::EmbeddingVector;
//! let vector = EmbeddingVector::new(vec![0.5, -0.25], Some(2))?;
//! assert_eq!(vector.dimensions(), 2);
//! # Ok::<(), forgesync_core::embedding::EmbeddingVectorError>(())
//! ```

use thiserror::Error;

/// A validated nonempty, finite, non-zero-norm embedding vector.
#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingVector {
    values: Vec<f32>,
}

impl EmbeddingVector {
    /// Validates a model vector, without normalizing it, and its expected dimension when supplied.
    ///
    /// # Errors
    ///
    /// Checks dimension range, expected dimension, finiteness, then norm; the first failure wins.
    pub fn new(
        values: Vec<f32>,
        expected_dimensions: Option<u32>,
    ) -> Result<Self, EmbeddingVectorError> {
        let dimensions =
            u32::try_from(values.len()).map_err(|_| EmbeddingVectorError::DimensionOutOfRange)?;
        if dimensions == 0 {
            return Err(EmbeddingVectorError::Empty);
        }
        if let Some(expected) = expected_dimensions
            && dimensions != expected
        {
            return Err(EmbeddingVectorError::WrongDimensions {
                expected,
                actual: dimensions,
            });
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(EmbeddingVectorError::NonFinite);
        }
        let squared_norm = values
            .iter()
            .map(|value| f64::from(*value) * f64::from(*value))
            .sum::<f64>();
        if squared_norm == 0.0 {
            return Err(EmbeddingVectorError::ZeroNorm);
        }

        Ok(Self { values })
    }

    /// Decodes exactly `dimensions` little-endian `f32` components and validates them.
    ///
    /// # Errors
    ///
    /// Returns [`EmbeddingVectorError::InvalidEncoding`] for zero dimensions or a length mismatch,
    /// and otherwise the same validation errors as [`Self::new`].
    pub fn from_little_endian(bytes: &[u8], dimensions: u32) -> Result<Self, EmbeddingVectorError> {
        let expected_bytes = usize::try_from(dimensions)
            .ok()
            .and_then(|dimensions| dimensions.checked_mul(4))
            .ok_or(EmbeddingVectorError::InvalidEncoding)?;
        if dimensions == 0 || bytes.len() != expected_bytes {
            return Err(EmbeddingVectorError::InvalidEncoding);
        }
        let values = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| f32::from_le_bytes(*chunk))
            .collect();
        Self::new(values, Some(dimensions))
    }

    pub fn dimensions(&self) -> u32 {
        u32::try_from(self.values.len()).expect("validated vector dimensions fit u32")
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// Encodes components as little-endian `f32` bytes with no header or dimension prefix.
    pub fn to_little_endian(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.values.len() * 4);
        for value in &self.values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }
}

/// A vector value failed dimension, numeric, or storage validation.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum EmbeddingVectorError {
    #[error("embedding vector is empty")]
    Empty,
    #[error("embedding vector dimension is out of range")]
    DimensionOutOfRange,
    #[error("embedding vector has {actual} dimensions; expected {expected}")]
    WrongDimensions { expected: u32, actual: u32 },
    #[error("embedding vector contains a non-finite component")]
    NonFinite,
    #[error("embedding vector has zero norm")]
    ZeroNorm,
    #[error("embedding vector bytes do not match the declared dimensions")]
    InvalidEncoding,
}

#[cfg(test)]
mod tests {
    //! Vector validation errors and the little-endian archive codec.

    use crate::embedding::{EmbeddingVector, EmbeddingVectorError};

    #[test]
    fn little_endian_vector_round_trips_with_explicit_dimensions() {
        let vector = EmbeddingVector::new(vec![0.5, -1.25, 3.0], Some(3)).expect("vector");
        let bytes = vector.to_little_endian();
        assert_eq!(bytes, [0, 0, 0, 63, 0, 0, 160, 191, 0, 0, 64, 64]);
        assert_eq!(
            EmbeddingVector::from_little_endian(&bytes, 3).expect("decoded vector"),
            vector
        );
    }

    #[rstest::rstest]
    #[case::empty(vec![], None, EmbeddingVectorError::Empty)]
    #[case::dimension(vec![1.0, 2.0], Some(3), EmbeddingVectorError::WrongDimensions { expected: 3, actual: 2 })]
    #[case::zero_norm(vec![0.0, 0.0], None, EmbeddingVectorError::ZeroNorm)]
    #[case::nan(vec![f32::NAN], None, EmbeddingVectorError::NonFinite)]
    #[case::infinity(vec![f32::INFINITY], None, EmbeddingVectorError::NonFinite)]
    fn invalid_model_vectors_have_specific_errors(
        #[case] values: Vec<f32>,
        #[case] dimensions: Option<u32>,
        #[case] expected: EmbeddingVectorError,
    ) {
        assert_eq!(EmbeddingVector::new(values, dimensions), Err(expected));
    }

    #[rstest::rstest]
    #[case::zero_dimension(&[], 0)]
    #[case::truncated(&[0, 0, 0, 0], 2)]
    #[case::trailing_byte(&[0, 0, 0, 0, 1], 1)]
    fn invalid_archive_lengths_are_rejected(#[case] bytes: &[u8], #[case] dimensions: u32) {
        assert_eq!(
            EmbeddingVector::from_little_endian(bytes, dimensions),
            Err(EmbeddingVectorError::InvalidEncoding)
        );
    }
}
