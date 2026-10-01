//! UTC microsecond instants shared by source clocks and archive metadata.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Errors returned when parsing or storing an RFC 3339 timestamp.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TimestampError {
    #[error("invalid RFC 3339 timestamp")]
    InvalidRfc3339,
    #[error("timestamp is outside the supported Unix-microsecond range")]
    OutOfRange,
    #[error("timestamp cannot be formatted as RFC 3339")]
    FormatFailure,
}

/// An absolute timestamp normalized to UTC at archive microsecond precision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UtcTimestamp(i64);

impl UtcTimestamp {
    /// Parses RFC 3339 and stores the instant in UTC microseconds.
    ///
    /// Sub-microsecond precision is truncated toward the Unix epoch. Preserve the original source
    /// spelling in provider data when that additional precision is relevant.
    pub fn parse(value: &str) -> Result<Self, TimestampError> {
        let timestamp =
            OffsetDateTime::parse(value, &Rfc3339).map_err(|_| TimestampError::InvalidRfc3339)?;
        let microseconds = timestamp.unix_timestamp_nanos() / 1_000;
        let microseconds = i64::try_from(microseconds).map_err(|_| TimestampError::OutOfRange)?;
        Ok(Self(microseconds))
    }

    /// Checks that signed Unix microseconds are representable and formattable as RFC 3339.
    ///
    /// # Errors
    ///
    /// Returns [`TimestampError::OutOfRange`] for unsupported instants and
    /// [`TimestampError::FormatFailure`] when RFC 3339 cannot represent the checked instant.
    pub fn from_unix_microseconds(value: i64) -> Result<Self, TimestampError> {
        let nanos = i128::from(value)
            .checked_mul(1_000)
            .ok_or(TimestampError::OutOfRange)?;
        OffsetDateTime::from_unix_timestamp_nanos(nanos).map_err(|_| TimestampError::OutOfRange)?;
        let timestamp = Self(value);
        timestamp.format_rfc3339()?;
        Ok(timestamp)
    }

    /// Returns signed microseconds since the Unix epoch.
    pub fn unix_microseconds(self) -> i64 {
        self.0
    }

    /// Formats the instant as RFC 3339 UTC text.
    ///
    /// # Errors
    ///
    /// Returns [`TimestampError::OutOfRange`] if reconstruction is unsupported, or
    /// [`TimestampError::FormatFailure`] when the instant cannot be expressed as RFC 3339.
    pub fn format_rfc3339(self) -> Result<String, TimestampError> {
        let nanos = i128::from(self.0)
            .checked_mul(1_000)
            .ok_or(TimestampError::OutOfRange)?;
        OffsetDateTime::from_unix_timestamp_nanos(nanos)
            .map_err(|_| TimestampError::OutOfRange)?
            .format(&Rfc3339)
            .map_err(|_| TimestampError::FormatFailure)
    }
}

impl Serialize for UtcTimestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = self.format_rfc3339().map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for UtcTimestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests;
