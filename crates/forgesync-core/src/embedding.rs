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
    /// order is preserved; this method does not normalize the vector's length. Components remain
    /// unchanged, including finite magnitudes larger or smaller than unit length.
    ///
    /// # Errors
    ///
    /// Validation checks representable/nonempty dimension first, then the optional expected
    /// dimension, then finite components and nonzero norm. When multiple conditions are invalid,
    /// the first applicable check determines the error. Matching dimensions alone does not prove
    /// that two vectors belong to the same service/model space.
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

    /// Returns the validated nonzero component count in the archive's dimension unit.
    ///
    /// This describes shape only; it does not identify the model or prove compatibility with
    /// another vector of the same dimension.
    pub fn dimensions(&self) -> u32 {
        u32::try_from(self.values.len()).expect("validated vector dimensions fit u32")
    }

    /// Borrows the original finite components in model order without normalization.
    ///
    /// The immutable slice preserves constructor validation. Similarity calculations may use
    /// magnitude-independent cosine math, but this value does not store unit-length components.
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
    //! # Validated vector and archive encoding boundaries
    //!
    //! The round-trip case fixes the vector's dimensions and little-endian byte representation.
    //! Named model-input cases distinguish empty, mismatched, zero-norm, and nonfinite vectors.
    //! Named archive-input cases reject zero dimensions, truncated components, and trailing bytes.
    //! Each rejection compares the specific typed error rather than accepting any failure.
    //!
    //! Inputs are local values with no model service or SQLite setup. Engine tests own request
    //! retries and model-response handling; store tests own persisted vector compatibility.
    //! This suite establishes the core value and codec contracts those boundaries depend on.
    //! Small direct cases stay near the implementation and need no workflow fixture.

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
