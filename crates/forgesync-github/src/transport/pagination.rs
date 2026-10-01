//! Parse the `rel="next"` target from GitHub REST Link headers.
//!
//! GitHub emits `<url>; rel="next", <url>; rel="last"`. The returned URL is resolved against the
//! request URL but not origin-checked here; every request validates its destination on entry.

use reqwest::Url;
use reqwest::header::{HeaderMap, LINK};

use crate::error::GitHubError;

/// Extracts the next-page destination from provider Link headers.
pub fn next_page_from_headers(
    current_url: &Url,
    headers: &HeaderMap,
) -> Result<Option<Url>, GitHubError> {
    for value in headers.get_all(LINK) {
        let value = value
            .to_str()
            .map_err(|_| GitHubError::InvalidPaginationLink)?;
        if let Some(target) = next_link_target(value)? {
            let next = current_url
                .join(target)
                .map_err(|_| GitHubError::InvalidPaginationLink)?;
            return Ok(Some(next));
        }
    }
    Ok(None)
}

/// Returns the `<...>` target of the entry whose parameters include `rel="next"`.
fn next_link_target(value: &str) -> Result<Option<&str>, GitHubError> {
    for entry in value.split(',') {
        let mut parts = entry.split(';');
        let target = parts.next().unwrap_or_default().trim();
        if !parts.any(|parameter| parameter.trim().eq_ignore_ascii_case(r#"rel="next""#)) {
            continue;
        }
        return target
            .strip_prefix('<')
            .and_then(|target| target.strip_suffix('>'))
            .filter(|target| !target.is_empty())
            .map(Some)
            .ok_or(GitHubError::InvalidPaginationLink);
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::next_link_target;
    use crate::error::GitHubError;

    #[test]
    fn selects_the_next_entry() {
        let header = r#"<https://api.github.com/x?page=1>; rel="prev", <https://api.github.com/x?page=3>; rel="next", <https://api.github.com/x?page=9>; rel="last""#;
        assert_eq!(
            next_link_target(header),
            Ok(Some("https://api.github.com/x?page=3"))
        );
    }

    #[test]
    fn missing_next_is_terminal() {
        assert_eq!(next_link_target(r#"<?page=1>; rel="first""#), Ok(None));
        assert_eq!(next_link_target(""), Ok(None));
    }

    #[test]
    fn next_without_a_bracketed_target_is_invalid() {
        assert_eq!(
            next_link_target(r#"?page=2; rel="next""#),
            Err(GitHubError::InvalidPaginationLink)
        );
        assert_eq!(
            next_link_target(r#"<>; rel="next""#),
            Err(GitHubError::InvalidPaginationLink)
        );
    }
}
