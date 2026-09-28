use std::str::FromStr;

use forgesync_core::{GitHubHost, ThreadNumber};
use thiserror::Error;

/// A repository name supplied to a local query.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RepositorySelector {
    host: GitHubHost,
    owner: String,
    name: String,
}

impl RepositorySelector {
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
}

impl FromStr for RepositorySelector {
    type Err = ReferenceParseError;

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

/// A thread reference accepted by local inspect commands.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ThreadSelector {
    repository: RepositorySelector,
    number: ThreadNumber,
}

impl ThreadSelector {
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

fn parse_thread_number(value: &str) -> Result<ThreadNumber, ReferenceParseError> {
    let number = value
        .parse::<u64>()
        .map_err(|_| ReferenceParseError::InvalidThread)?;
    ThreadNumber::new(number).map_err(|_| ReferenceParseError::InvalidThread)
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| !character.is_whitespace() && !character.is_control())
        && !value.contains(['/', '#', '?', '@', '\\'])
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{ReferenceParseError, RepositorySelector, ThreadSelector};

    #[test]
    fn parses_public_and_enterprise_repository_selectors() {
        let public = RepositorySelector::from_str("Example/Project").expect("public repo");
        assert_eq!(public.host().as_str(), "github.com");
        assert_eq!(public.owner(), "Example");
        assert_eq!(public.name(), "Project");

        let enterprise = RepositorySelector::from_str("https://GHE.example.test/org/repo")
            .expect("enterprise repo");
        assert_eq!(enterprise.host().as_str(), "ghe.example.test");
    }

    #[test]
    fn parses_issue_and_pull_request_references() {
        let issue = ThreadSelector::from_str("org/repo#42").expect("issue reference");
        assert_eq!(issue.number().get(), 42);
        assert_eq!(issue.repository().host().as_str(), "github.com");

        let pull = ThreadSelector::from_str("https://ghe.example.test/org/repo/pull/7?x=1")
            .expect("pull request URL");
        assert_eq!(pull.number().get(), 7);
        assert_eq!(pull.repository().host().as_str(), "ghe.example.test");
    }

    #[test]
    fn rejects_ambiguous_or_unsafe_references() {
        for value in [
            "repo",
            "org/repo#0",
            "org/repo#1/2",
            "http://github.com/a/b",
        ] {
            assert!(
                ThreadSelector::from_str(value).is_err(),
                "accepted malformed thread selector {value}"
            );
        }
        assert_eq!(
            RepositorySelector::from_str("org/repo/extra"),
            Err(ReferenceParseError::InvalidRepository)
        );
    }
}
