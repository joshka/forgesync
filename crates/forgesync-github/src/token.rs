//! Credential value passed from the application boundary to GitHub transport.
//!
//! [`GitHubToken`] rejects an empty or whitespace-containing value. Its `Debug` implementation
//! redacts the secret, while [`GitHubToken::expose`] deliberately returns it for an authorization
//! header. Redacted debug output does not make other formatting or accidental logging safe.
//!
//! The CLI resolves credentials from its configured sources and constructs this type. The
//! transport attaches it only to destinations accepted by its trusted-origin checks. Resource
//! fetching never reads process environment variables directly.
//!
//! Use this module when building a provider client, not when deciding which credential source
//! wins. Keep token handling separate from repository and host identity in
//! `forgesync-core::identity`.

use std::fmt;

/// A GitHub credential that is redacted from debug output.
///
/// Create this at the application boundary and pass it to the transport client. Debug formatting
/// hides the secret; [`Self::expose`] deliberately returns it for an authorization header, so
/// callers must keep that value out of logs and error messages.
///
/// # Examples
///
/// ```
/// use forgesync_github::token::GitHubToken;
///
/// let token = GitHubToken::new("example-token")?;
/// assert_eq!(format!("{token:?}"), "GitHubToken([REDACTED])");
/// # Ok::<(), forgesync_github::token::GitHubTokenError>(())
/// ```
#[derive(Clone, Eq, PartialEq)]
pub struct GitHubToken(String);

impl GitHubToken {
    /// Creates a token after checking that it is non-empty and contains no whitespace.
    pub fn new(value: impl Into<String>) -> Result<Self, GitHubTokenError> {
        let value = value.into();
        if value.is_empty()
            || value
                .chars()
                .any(|character| character.is_whitespace() || character.is_control())
        {
            return Err(GitHubTokenError);
        }
        Ok(Self(value))
    }

    /// Returns the raw token for the HTTP authorization header.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for GitHubToken {
    /// Writes a fixed redaction marker so formatting a token cannot reveal its credential contents.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GitHubToken([REDACTED])")
    }
}

/// Invalid GitHub token input.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("GitHub token must be non-empty and contain no whitespace")]
pub struct GitHubTokenError;

#[cfg(test)]
mod tests {
    use super::GitHubToken;

    #[test]
    fn token_debug_output_is_redacted() {
        let token = GitHubToken::new("secret-token-value").expect("valid token");
        assert_eq!(format!("{token:?}"), "GitHubToken([REDACTED])");
        assert!(!format!("{token:?}").contains("secret-token-value"));
    }
}
