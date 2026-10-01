//! Validated GitHub API authority used in provider and archive identity.
//!
//! This is the domain identity, not the transport authorization check: the GitHub transport
//! independently validates request and pagination URLs against its configured origin.

use std::fmt;
use std::net::Ipv6Addr;

use serde::{Deserialize, Serialize};

use super::IdentityError;

/// Canonical GitHub host identity, stored as a lower-case authority without a scheme.
///
/// Accepts `github.com`, `https://github.com`, and HTTPS enterprise authorities with an optional
/// port. It rejects paths so an API base path cannot accidentally create a second host identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct GitHubHost(String);

impl GitHubHost {
    /// Checks and canonicalizes a public or enterprise HTTPS authority without contacting it.
    ///
    /// Trims outer whitespace and accepts a bare authority or case-insensitive `https://` origin.
    /// A single trailing slash is accepted on the origin form. DNS spelling becomes ASCII
    /// lowercase, a trailing DNS dot is removed, and port 443 is omitted. Bracketed IPv6
    /// addresses use their canonical address spelling. Non-default positive ports are retained.
    ///
    /// # Errors
    ///
    /// Returns [`IdentityError::InvalidGitHubHost`] for unsupported schemes, credentials, paths,
    /// query/fragment syntax, malformed authorities or labels, and unusable ports. DNS names must
    /// already use ASCII spelling; this parser does not perform IDNA conversion or DNS lookup.
    ///
    /// ```
    /// use forgesync_core::identity::{GitHubHost, IdentityError};
    ///
    /// let host = GitHubHost::parse(" https://GHE.Example.test.:443/ ")?;
    /// assert_eq!(host.as_str(), "ghe.example.test");
    /// assert_eq!(host.https_origin(), "https://ghe.example.test");
    /// assert_eq!(
    ///     GitHubHost::parse("http://github.com"),
    ///     Err(IdentityError::InvalidGitHubHost)
    /// );
    /// # Ok::<(), IdentityError>(())
    /// ```
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

    /// Builds the HTTPS origin, without an API path or trailing slash.
    pub fn https_origin(&self) -> String {
        format!("https://{}", self.0)
    }
}

impl fmt::Display for GitHubHost {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for GitHubHost {
    type Error = IdentityError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<GitHubHost> for String {
    fn from(value: GitHubHost) -> Self {
        value.0
    }
}

/// Canonicalizes a bracketed IPv6 authority and omits the default HTTPS port.
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

/// Normalizes a DNS authority before it becomes part of persistent repository identity.
/// A trailing dot and the default HTTPS port must not create distinct hosts.
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

/// Accepts an optional authority port without allowing path or credential syntax.
fn parse_port_suffix(suffix: &str) -> Result<Option<u16>, IdentityError> {
    if suffix.is_empty() {
        return Ok(None);
    }
    let port_text = suffix
        .strip_prefix(':')
        .ok_or(IdentityError::InvalidGitHubHost)?;
    parse_port(port_text).map(Some)
}

/// Rejects zero and out-of-range ports before the host becomes an identity.
fn parse_port(value: &str) -> Result<u16, IdentityError> {
    let port = value
        .parse::<u16>()
        .map_err(|_| IdentityError::InvalidGitHubHost)?;
    if port == 0 {
        return Err(IdentityError::InvalidGitHubHost);
    }
    Ok(port)
}
