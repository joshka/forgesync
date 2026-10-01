//! Parse user-facing repository and thread selectors.
//!
//! Parsing proves shape, not existence, permission, or current naming. Selectors keep the supplied
//! owner/name spelling and carry no provider IDs, so resolve them before using durable identity for
//! storage or deduplication. HTTPS input is host plus literal path segments: query, fragment, and
//! trailing slashes are dropped and percent escapes are not decoded. Plain `owner/name` uses
//! `github.com`.

use std::str::FromStr;

use forgesync_core::content::Repository;
use forgesync_core::identity::{GitHubHost, ThreadNumber};
use thiserror::Error;

/// A repository name supplied to a local query.
///
/// Parse an `owner/repository` pair for the default `github.com` host, or an HTTPS URL for an
/// explicit host.
///
/// # Examples
///
/// ```
/// use std::str::FromStr;
///
/// use forgesync_engine::reference::RepositorySelector;
///
/// let repository = RepositorySelector::from_str("owner/project")?;
/// assert_eq!(repository.as_url(), "https://github.com/owner/project");
/// # Ok::<(), forgesync_engine::reference::ReferenceParseError>(())
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RepositorySelector {
    /// Normalized host for lookup; no provider repository identity is encoded.
    host: GitHubHost,
    /// Supplied display owner spelling, retained without case folding.
    owner: String,
    /// Supplied display repository spelling, retained without percent decoding.
    name: String,
}

impl RepositorySelector {
    /// Copies the normalized host and current display path from a repository record.
    pub fn from_repository(repository: &Repository) -> Self {
        Self {
            host: repository.id.host().clone(),
            owner: repository.owner.clone(),
            name: repository.name.clone(),
        }
    }

    /// Returns the selected GitHub host.
    pub fn host(&self) -> &GitHubHost {
        &self.host
    }

    /// Returns the current owner name.
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// Returns the current repository name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Formats the retained host/display path as HTTPS text for run scope and diagnostics.
    pub fn as_url(&self) -> String {
        format!(
            "https://{}/{}/{}",
            self.host.as_str(),
            self.owner,
            self.name
        )
    }
}

impl FromStr for RepositorySelector {
    type Err = ReferenceParseError;

    /// Parses a default-host repository pair or explicit HTTPS repository URL.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let default_host =
            GitHubHost::parse("github.com").map_err(|_| ReferenceParseError::InvalidRepository)?;
        let (host, path) = match parse_https_path(value)? {
            Some((host, path)) => (host, path),
            None => (default_host, value),
        };
        let mut segments = path.split('/');
        let owner = segments.next().unwrap_or_default();
        let name = segments.next().unwrap_or_default();
        if segments.next().is_some() || !valid_segment(owner) || !valid_segment(name) {
            return Err(ReferenceParseError::InvalidRepository);
        }
        Ok(Self {
            host,
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }
}

/// Checked repository display path and positive local number used to request a thread.
///
/// Issue and pull URLs produce the same shape; route spelling does not retain thread kind.
///
/// ```
/// use forgesync_engine::reference::ThreadSelector;
///
/// let selector: ThreadSelector = "owner/project#42".parse()?;
/// assert_eq!(selector.number().get(), 42);
/// assert_eq!(selector.repository().name(), "project");
/// # Ok::<(), forgesync_engine::reference::ReferenceParseError>(())
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ThreadSelector {
    /// Display-path lookup coordinates, without a stable provider repository ID.
    repository: RepositorySelector,
    /// Checked positive number within the selected repository.
    number: ThreadNumber,
}

impl ThreadSelector {
    /// Combines repository lookup coordinates with a checked positive thread number.
    pub fn new(repository: RepositorySelector, number: ThreadNumber) -> Self {
        Self { repository, number }
    }

    /// Returns the selected repository.
    pub fn repository(&self) -> &RepositorySelector {
        &self.repository
    }

    /// Returns the positive issue or pull-request number.
    pub fn number(&self) -> ThreadNumber {
        self.number
    }
}

impl FromStr for ThreadSelector {
    type Err = ReferenceParseError;

    /// Parses a repository-qualified discussion number or issue/pull-request URL.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.contains("://") {
            return parse_thread_url(value);
        }
        let (repository, number) = value
            .rsplit_once('#')
            .ok_or(ReferenceParseError::InvalidThread)?;
        let repository = repository.parse()?;
        let number = parse_thread_number(number)?;
        Ok(Self { repository, number })
    }
}

/// A malformed repository or thread reference.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ReferenceParseError {
    /// A repository selector must be `OWNER/REPO` or an HTTPS repository URL.
    #[error("repository reference must be OWNER/REPO or an HTTPS repository URL")]
    InvalidRepository,
    /// A thread selector must be `OWNER/REPO#NUMBER` or an HTTPS issue or pull URL.
    #[error("thread reference must be OWNER/REPO#NUMBER or an HTTPS issue or pull URL")]
    InvalidThread,
}

/// Extracts a checked thread selector from a GitHub HTTPS URL.
fn parse_thread_url(value: &str) -> Result<ThreadSelector, ReferenceParseError> {
    let (host, path) = parse_https_path(value)?.ok_or(ReferenceParseError::InvalidThread)?;
    let mut segments = path.split('/');
    let owner = segments.next().unwrap_or_default();
    let name = segments.next().unwrap_or_default();
    let route = segments.next().unwrap_or_default();
    let number = segments.next().unwrap_or_default();
    if segments.next().is_some()
        || !valid_segment(owner)
        || !valid_segment(name)
        || !matches!(route, "issues" | "pull")
    {
        return Err(ReferenceParseError::InvalidThread);
    }
    Ok(ThreadSelector {
        repository: RepositorySelector {
            host,
            owner: owner.to_owned(),
            name: name.to_owned(),
        },
        number: parse_thread_number(number)?,
    })
}

/// Separates an explicit host from a selector path without provider I/O.
fn parse_https_path(value: &str) -> Result<Option<(GitHubHost, &str)>, ReferenceParseError> {
    if !value.contains("://") {
        return Ok(None);
    }
    let has_https_scheme = value
        .get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"));
    if !has_https_scheme {
        return Err(ReferenceParseError::InvalidRepository);
    }
    let url = &value[8..];
    let (authority, path) = url
        .split_once('/')
        .ok_or(ReferenceParseError::InvalidRepository)?;
    let host = GitHubHost::parse(authority).map_err(|_| ReferenceParseError::InvalidRepository)?;
    let path = path
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    Ok(Some((host, path)))
}

/// Rejects zero and malformed repository-local discussion numbers.
fn parse_thread_number(value: &str) -> Result<ThreadNumber, ReferenceParseError> {
    let number = value
        .parse::<u64>()
        .map_err(|_| ReferenceParseError::InvalidThread)?;
    ThreadNumber::new(number).map_err(|_| ReferenceParseError::InvalidThread)
}

/// Checks an owner or repository path segment before constructing a selector.
fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| !character.is_whitespace() && !character.is_control())
        && !value.contains(['/', '#', '?', '@', '\\'])
}

#[cfg(test)]
mod tests;
