//! Opaque provider IDs and checked commit revisions.
//!
//! [`ProviderId`] retains an identifier issued by GitHub without assuming it is numeric or
//! repository-local. [`CommitSha`] requires a full supported hexadecimal revision so pull-request
//! head context and review evidence can refer to the same revision unambiguously.
//!
//! The provider normalization layer constructs these values from REST or GraphQL responses.
//! Archive keys and display numbers are separate types in sibling identity modules. Do not derive
//! ordering from an opaque provider ID or infer a commit from a branch name.
//!
//! Use these constructors at the provider boundary; use their string views only for serialization,
//! transport parameters, or user-facing diagnostics that are safe to display.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::IdentityError;

/// A non-empty provider-issued ID, kept opaque because REST and GraphQL use different forms.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderId(String);

impl ProviderId {
    /// Creates an opaque provider ID without changing its spelling.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
        let value = value.into();
        if value.is_empty()
            || value
                .chars()
                .any(|character| character.is_whitespace() || character.is_control())
        {
            return Err(IdentityError::InvalidProviderId);
        }
        Ok(Self(value))
    }

    /// Returns the original provider ID.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    /// Displays the original opaque provider identifier without interpreting numeric-looking
    /// values.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for ProviderId {
    /// Encodes opaque provider identity as text, preserving IDs that are not numeric.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProviderId {
    /// Rechecks provider identity constraints when reading persisted text; invalid IDs cannot
    /// bypass construction.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Full Git commit object ID, supporting SHA-1 and SHA-256 repositories.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CommitSha(String);

impl CommitSha {
    /// Creates a lower-case commit ID from a full hexadecimal SHA-1 or SHA-256 value.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
        let value = value.into();
        if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(IdentityError::InvalidCommitSha);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    /// Returns the canonical lower-case hexadecimal commit ID.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommitSha {
    /// Displays the canonical lower-case full commit revision used to relate head and review
    /// evidence.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for CommitSha {
    /// Encodes the canonical full commit revision as text rather than an abbreviated display
    /// revision.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CommitSha {
    /// Validates a full supported commit revision and normalizes hexadecimal case during
    /// deserialization.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}
