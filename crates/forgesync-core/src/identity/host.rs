//! Host identities.

use std::fmt;
use std::net::Ipv6Addr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::IdentityError;

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
