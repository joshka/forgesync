//! Parse REST Link headers into trusted continuation candidates.
//!
//! The parser separates header entries without treating punctuation inside quoted values as a new
//! link. It selects the `next` relation and resolves that target against the request URL. The
//! client then validates the resulting destination against its configured origin before making
//! another request.
//!
//! Pagination is a transport concern: the engine should see an optional next-page URL rather than
//! parse header grammar. A malformed or untrusted link is a typed provider failure. Do not infer
//! that a missing next link proves a prior partial page set was committed; durable checkpoint and
//! family completeness remain engine and store responsibilities.

use reqwest::Url;
use reqwest::header::LINK;

use crate::error::GitHubError;
use crate::transport::TrustedOrigin;

/// Extracts a validated next-page destination from provider Link headers.
pub fn next_page_from_headers(
    current_url: &Url,
    headers: &reqwest::header::HeaderMap,
    origin: &TrustedOrigin,
) -> Result<Option<Url>, GitHubError> {
    for value in headers.get_all(LINK).iter() {
        let value = value
            .to_str()
            .map_err(|_| GitHubError::InvalidPaginationLink)?;
        for item in split_link_header(value) {
            if let Some(target) = next_link_target(item)? {
                let next = current_url
                    .join(target)
                    .map_err(|_| GitHubError::InvalidPaginationLink)?;
                origin.validate(&next)?;
                return Ok(Some(next));
            }
        }
    }
    Ok(None)
}

/// Separates Link entries without splitting inside quoted values.
pub fn split_link_header(value: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut start = 0;
    let mut in_angle = false;
    let mut in_quotes = false;
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if in_quotes {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_quotes = false;
            }
            continue;
        }
        match character {
            '<' => in_angle = true,
            '>' => in_angle = false,
            '"' => in_quotes = true,
            ',' if !in_angle => {
                items.push(value[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    if start <= value.len() {
        items.push(value[start..].trim());
    }
    items
}

/// Returns the target of a next relation from one Link entry.
pub fn next_link_target(item: &str) -> Result<Option<&str>, GitHubError> {
    let Some(close_bracket) = item.find('>') else {
        if item.to_ascii_lowercase().contains("rel=\"next\"") {
            return Err(GitHubError::InvalidPaginationLink);
        }
        return Ok(None);
    };
    let Some(target) = item.strip_prefix('<') else {
        return Err(GitHubError::InvalidPaginationLink);
    };
    let target = &target[..close_bracket - 1];
    let parameters = &item[close_bracket + 1..];
    let is_next = parameters.split(';').any(|parameter| {
        let Some((name, value)) = parameter.trim().split_once('=') else {
            return false;
        };
        if !name.trim().eq_ignore_ascii_case("rel") {
            return false;
        }
        value
            .trim()
            .trim_matches('"')
            .split_ascii_whitespace()
            .any(|relation| relation.eq_ignore_ascii_case("next"))
    });
    if is_next && target.is_empty() {
        return Err(GitHubError::InvalidPaginationLink);
    }
    Ok(is_next.then_some(target))
}
