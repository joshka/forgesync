//! Checked provider and archive identities.

use std::fmt;
use std::net::Ipv6Addr;
use std::num::NonZeroU64;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Errors returned when constructing a checked identity.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum IdentityError {
    /// A GitHub host was not a valid HTTPS host name or IP authority.
    #[error("GitHub host must be an HTTPS host name or IP address without credentials or a path")]
    InvalidGitHubHost,
    /// A provider-issued opaque ID was empty or contained whitespace/control characters.
    #[error("provider ID must be non-empty and contain no whitespace or control characters")]
    InvalidProviderId,
    /// A commit SHA did not use a supported full hexadecimal form.
    #[error("commit SHA must contain 40 or 64 hexadecimal characters")]
    InvalidCommitSha,
    /// A GitHub issue or pull request number must be positive.
    #[error("thread number must be greater than zero")]
    InvalidThreadNumber,
    /// A local archive run ID must be positive.
    #[error("run ID must be greater than zero")]
    InvalidRunId,
    /// An acquisition sequence must be positive.
    #[error("observation sequence must be greater than zero")]
    InvalidObservationSequence,
}

/// Canonical GitHub host identity, stored as a lower-case authority without a scheme.
///
/// Accepts `github.com`, `https://github.com`, and HTTPS enterprise authorities with an optional
/// port. It rejects paths so an API base path cannot accidentally create a second host identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GitHubHost(String);

impl GitHubHost {
    /// Parses a public or enterprise GitHub HTTPS host.
    pub fn parse(value: &str) -> Result<Self, IdentityError> {
        let input = value.trim();
        let authority = if input
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
        {
            let authority = &input[8..];
            authority.strip_suffix('/').unwrap_or(authority)
        } else if input.contains("://") {
            return Err(IdentityError::InvalidGitHubHost);
        } else {
            input
        };

        if authority.is_empty()
            || authority.chars().any(|character| {
                character.is_whitespace() || matches!(character, '/' | '?' | '#' | '@' | '\\')
            })
        {
            return Err(IdentityError::InvalidGitHubHost);
        }

        let canonical = if authority.starts_with('[') {
            canonicalize_ipv6_authority(authority)?
        } else {
            canonicalize_dns_authority(authority)?
        };

        Ok(Self(canonical))
    }

    /// Returns the canonical authority, such as `github.com` or `ghe.example.test:8443`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the HTTPS origin for this host.
    pub fn https_origin(&self) -> String {
        format!("https://{}", self.0)
    }
}

impl fmt::Display for GitHubHost {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for GitHubHost {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for GitHubHost {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

fn canonicalize_ipv6_authority(authority: &str) -> Result<String, IdentityError> {
    let closing_bracket = authority
        .find(']')
        .ok_or(IdentityError::InvalidGitHubHost)?;
    let address_text = &authority[1..closing_bracket];
    let address = address_text
        .parse::<Ipv6Addr>()
        .map_err(|_| IdentityError::InvalidGitHubHost)?;
    let suffix = &authority[closing_bracket + 1..];
    let port = parse_port_suffix(suffix)?.filter(|port| *port != 443);
    Ok(match port {
        Some(port) => format!("[{address}]:{port}"),
        None => format!("[{address}]"),
    })
}

fn canonicalize_dns_authority(authority: &str) -> Result<String, IdentityError> {
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => {
            let port = parse_port(port)?;
            (host, (port != 443).then_some(port))
        }
        Some(_) => return Err(IdentityError::InvalidGitHubHost),
        None => (authority, None),
    };

    let host = host.strip_suffix('.').unwrap_or(host);
    if host.is_empty() || host.len() > 253 || !host.is_ascii() {
        return Err(IdentityError::InvalidGitHubHost);
    }

    for label in host.split('.') {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(IdentityError::InvalidGitHubHost);
        }
    }

    let host = host.to_ascii_lowercase();
    Ok(match port {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

fn parse_port_suffix(suffix: &str) -> Result<Option<u16>, IdentityError> {
    if suffix.is_empty() {
        return Ok(None);
    }
    let port_text = suffix
        .strip_prefix(':')
        .ok_or(IdentityError::InvalidGitHubHost)?;
    parse_port(port_text).map(Some)
}

fn parse_port(value: &str) -> Result<u16, IdentityError> {
    let port = value
        .parse::<u16>()
        .map_err(|_| IdentityError::InvalidGitHubHost)?;
    if port == 0 {
        return Err(IdentityError::InvalidGitHubHost);
    }
    Ok(port)
}

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
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for ProviderId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProviderId {
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
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for CommitSha {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CommitSha {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Positive issue or pull request number scoped to one repository.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ThreadNumber(NonZeroU64);

impl ThreadNumber {
    /// Creates a checked positive thread number.
    pub fn new(value: u64) -> Result<Self, IdentityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(IdentityError::InvalidThreadNumber)
    }

    /// Returns the underlying number.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl Serialize for ThreadNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.get())
    }
}

impl<'de> Deserialize<'de> for ThreadNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Host-qualified stable repository identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RepositoryId {
    host: GitHubHost,
    provider_id: ProviderId,
}

impl RepositoryId {
    /// Creates a repository identity from checked host and provider IDs.
    pub fn new(host: GitHubHost, provider_id: ProviderId) -> Self {
        Self { host, provider_id }
    }

    /// Returns the provider host.
    pub fn host(&self) -> &GitHubHost {
        &self.host
    }

    /// Returns the stable provider ID.
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }
}

/// Stable issue or pull request identity scoped to its repository.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ThreadId {
    repository: RepositoryId,
    provider_id: ProviderId,
    number: ThreadNumber,
}

impl ThreadId {
    /// Creates a thread identity with its host-qualified parent and display number.
    pub fn new(repository: RepositoryId, provider_id: ProviderId, number: ThreadNumber) -> Self {
        Self {
            repository,
            provider_id,
            number,
        }
    }

    /// Returns the containing repository identity.
    pub fn repository(&self) -> &RepositoryId {
        &self.repository
    }

    /// Returns the stable provider ID.
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    /// Returns the issue or pull request number.
    pub fn number(&self) -> ThreadNumber {
        self.number
    }
}

/// Stable comment identity scoped to a discussion.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CommentId {
    thread: ThreadId,
    provider_id: ProviderId,
}

impl CommentId {
    /// Creates a comment identity with its parent thread and provider ID.
    pub fn new(thread: ThreadId, provider_id: ProviderId) -> Self {
        Self {
            thread,
            provider_id,
        }
    }

    /// Returns the parent discussion identity.
    pub fn thread(&self) -> &ThreadId {
        &self.thread
    }

    /// Returns the stable provider ID.
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }
}

/// Stable pull request review identity scoped to a discussion.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ReviewId {
    thread: ThreadId,
    provider_id: ProviderId,
}

impl ReviewId {
    /// Creates a review identity with its parent pull request and provider ID.
    pub fn new(thread: ThreadId, provider_id: ProviderId) -> Self {
        Self {
            thread,
            provider_id,
        }
    }

    /// Returns the parent pull request identity.
    pub fn thread(&self) -> &ThreadId {
        &self.thread
    }

    /// Returns the stable provider ID.
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }
}

/// Stable review-thread identity scoped to a pull request.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ReviewThreadId {
    thread: ThreadId,
    provider_id: ProviderId,
}

impl ReviewThreadId {
    /// Creates a review-thread identity with its parent pull request and provider ID.
    pub fn new(thread: ThreadId, provider_id: ProviderId) -> Self {
        Self {
            thread,
            provider_id,
        }
    }

    /// Returns the parent pull request identity.
    pub fn thread(&self) -> &ThreadId {
        &self.thread
    }

    /// Returns the stable provider ID.
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }
}

/// Positive local archive run identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RunId(NonZeroU64);

impl RunId {
    /// Creates a checked positive run ID.
    pub fn new(value: u64) -> Result<Self, IdentityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(IdentityError::InvalidRunId)
    }

    /// Returns the underlying ID.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl Serialize for RunId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.get())
    }
}

impl<'de> Deserialize<'de> for RunId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Positive acquisition sequence assigned before a family request starts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObservationSequence(NonZeroU64);

impl ObservationSequence {
    /// Creates a checked positive observation sequence.
    pub fn new(value: u64) -> Result<Self, IdentityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(IdentityError::InvalidObservationSequence)
    }

    /// Returns the underlying sequence.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl Serialize for ObservationSequence {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.get())
    }
}

impl<'de> Deserialize<'de> for ObservationSequence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// A parsed local thread reference after its repository scope is known.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ThreadReference {
    repository: RepositoryId,
    number: ThreadNumber,
}

impl ThreadReference {
    /// Creates a repository-qualified thread reference.
    pub fn new(repository: RepositoryId, number: ThreadNumber) -> Self {
        Self { repository, number }
    }

    /// Returns the host-qualified repository.
    pub fn repository(&self) -> &RepositoryId {
        &self.repository
    }

    /// Returns the issue or pull request number.
    pub fn number(&self) -> ThreadNumber {
        self.number
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        CommitSha, GitHubHost, IdentityError, ProviderId, RepositoryId, RunId, ThreadNumber,
    };

    #[test]
    fn host_identity_normalizes_authorities_and_rejects_paths() {
        let host = GitHubHost::parse("HTTPS://GHE.Example.Test:8443/").expect("valid host");
        assert_eq!(host.as_str(), "ghe.example.test:8443");
        assert_eq!(host.https_origin(), "https://ghe.example.test:8443");
        assert_eq!(
            GitHubHost::parse("https://[2001:DB8::1]:443")
                .expect("IPv6 host")
                .as_str(),
            "[2001:db8::1]"
        );
        assert_eq!(
            GitHubHost::parse("github.com:443")
                .expect("default port")
                .as_str(),
            "github.com"
        );

        for value in [
            "http://github.com",
            "https://github.com/api/v3",
            "https://user:pass@github.com",
            "github..com",
            "bad host",
            "github.com:0",
            "[not-ipv6]",
        ] {
            assert_eq!(
                GitHubHost::parse(value),
                Err(IdentityError::InvalidGitHubHost),
                "accepted invalid host {value}"
            );
        }
    }

    #[test]
    fn provider_and_numeric_identities_reject_invalid_values() {
        assert_eq!(ProviderId::new(" "), Err(IdentityError::InvalidProviderId));
        assert_eq!(
            ProviderId::new("bad\nid"),
            Err(IdentityError::InvalidProviderId)
        );
        assert_eq!(
            ThreadNumber::new(0),
            Err(IdentityError::InvalidThreadNumber)
        );
        assert_eq!(RunId::new(0), Err(IdentityError::InvalidRunId));
        assert_eq!(
            CommitSha::new("deadbeef"),
            Err(IdentityError::InvalidCommitSha)
        );
        assert_eq!(
            CommitSha::new("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
                .expect("full SHA-1")
                .as_str(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );

        let host = GitHubHost::parse("github.com").expect("host");
        let provider_id = ProviderId::new("R_fixture_41").expect("provider ID");
        let id = RepositoryId::new(host, provider_id);
        let value = serde_json::to_value(&id).expect("serialize repository ID");
        let decoded: RepositoryId = serde_json::from_value(value.clone()).expect("deserialize ID");
        assert_eq!(decoded, id);

        let invalid: Result<GitHubHost, _> = serde_json::from_value(json!("http://github.com"));
        assert!(invalid.is_err(), "deserialization must use host validation");
    }
}
