//! Validated vectors at the boundary of semantic retrieval.
//!
//! [`EmbeddingVector`] owns finite, nonempty, nonzero-norm `f32` components. Construct it with
//! [`EmbeddingVector::new`] after receiving service output, optionally checking the model's
//! expected dimension. The little-endian methods encode the archive representation and validate it
//! again on read. [`EmbeddingVectorError`] distinguishes malformed dimensions, numeric values, and
//! bytes.
//!
//! The vector does not carry a model name or document recipe. The engine and store pair it with
//! that metadata before exact similarity comparison, so vectors from incompatible spaces are not
//! mixed. Validation here keeps later search math from silently ranking NaN or empty inputs.
//!
//! ```
//! use forgesync_core::embedding::EmbeddingVector;
//! let vector = EmbeddingVector::new(vec![0.5, -0.25], Some(2))?;
//! assert_eq!(vector.dimensions(), 2);
//! # Ok::<(), forgesync_core::embedding::EmbeddingVectorError>(())
//! ```

use thiserror::Error;

/// A validated finite, non-zero-norm embedding vector.
///
/// Construct this from service output before storing it or computing similarity. The expected
/// dimension is optional when decoding an existing vector, but should be supplied when a model
/// declares one.
///
/// # Examples
///
/// ```
/// use forgesync_core::embedding::EmbeddingVector;
///
/// let vector = EmbeddingVector::new(vec![0.5, -0.25], Some(2))?;
/// assert_eq!(vector.dimensions(), 2);
/// assert_eq!(vector.values(), &[0.5, -0.25]);
/// # Ok::<(), forgesync_core::embedding::EmbeddingVectorError>(())
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingVector {
    /// Finite model-order components with validated nonempty dimension and nonzero squared norm.
    values: Vec<f32>,
}

impl EmbeddingVector {
    /// Validates a model vector and, when supplied, its expected dimension.
    ///
    /// Empty, non-finite, zero-norm, and dimension-mismatched vectors are rejected. The input
    /// order is preserved; this method does not normalize the vector's length.
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

    /// Decodes exactly `dimensions` little-endian IEEE 754 `f32` components and validates them.
    ///
    /// The byte count must equal four times a nonzero dimension. Decoding preserves component order
    /// and magnitude; it does not normalize vectors or check service/model compatibility metadata.
    /// That metadata belongs to the store/engine record carrying this value.
    ///
    /// # Errors
    ///
    /// Returns [`EmbeddingVectorError::InvalidEncoding`] for zero dimensions, an unrepresentable
    /// byte count, or a length mismatch. Decoded non-finite or zero-norm values produce the same
    /// validation errors as [`Self::new`].
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

    /// Returns the number of components.
    pub fn dimensions(&self) -> u32 {
        u32::try_from(self.values.len()).expect("validated vector dimensions fit u32")
    }

    /// Returns vector components in model order.
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// Encodes model-order components as consecutive little-endian IEEE 754 `f32` bytes.
    ///
    /// The returned buffer contains exactly four bytes per component, without a header or dimension
    /// prefix. Store the dimension and compatibility metadata alongside it; decoding requires the
    /// explicit dimension and revalidates numeric content.
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
    /// An embedding response contained no components.
    #[error("embedding vector is empty")]
    Empty,
    /// The number of components cannot be represented by the archive format.
    #[error("embedding vector dimension is out of range")]
    DimensionOutOfRange,
    /// The vector has a different dimension than the configured model dimension.
    #[error("embedding vector has {actual} dimensions; expected {expected}")]
    WrongDimensions {
        /// Configured or otherwise required dimensions.
        expected: u32,
        /// Returned dimensions.
        actual: u32,
    },
    /// The vector contains NaN or an infinite component.
    #[error("embedding vector contains a non-finite component")]
    NonFinite,
    /// The vector has no direction because all components are zero.
    #[error("embedding vector has zero norm")]
    ZeroNorm,
    /// The persisted bytes do not match the declared dimensions.
    #[error("embedding vector bytes do not match the declared dimensions")]
    InvalidEncoding,
}

#[cfg(test)]
mod tests {
    use super::{EmbeddingVector, EmbeddingVectorError};

    #[test]
    fn little_endian_vector_round_trips_with_explicit_dimensions() {
        let vector = EmbeddingVector::new(vec![0.5, -1.25, 3.0], Some(3)).expect("vector");
        let bytes = vector.to_little_endian();
        assert_eq!(bytes.len(), 12);
        assert_eq!(
            EmbeddingVector::from_little_endian(&bytes, 3).expect("decoded vector"),
            vector
        );
    }

    #[test]
    fn invalid_dimensions_and_numeric_values_are_rejected() {
        assert_eq!(
            EmbeddingVector::new(vec![1.0, 2.0], Some(3)),
            Err(EmbeddingVectorError::WrongDimensions {
                expected: 3,
                actual: 2
            })
        );
        assert_eq!(
            EmbeddingVector::new(vec![0.0, 0.0], None),
            Err(EmbeddingVectorError::ZeroNorm)
        );
        assert_eq!(
            EmbeddingVector::new(vec![f32::NAN], None),
            Err(EmbeddingVectorError::NonFinite)
        );
        assert_eq!(
            EmbeddingVector::from_little_endian(&[0, 0, 0, 0], 2),
            Err(EmbeddingVectorError::InvalidEncoding)
        );
    }
}
