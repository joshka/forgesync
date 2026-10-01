//! Selector grammar examples.

use crate::reference::{ReferenceParseError, RepositorySelector, ThreadSelector};

#[test]
fn plain_repository_uses_default_host_and_preserves_display_case() {
    let selector: RepositorySelector = "Example/Project".parse().expect("repository pair");
    assert_eq!(selector.host().as_str(), "github.com");
    assert_eq!(selector.owner(), "Example");
    assert_eq!(selector.name(), "Project");
}

#[test]
fn enterprise_repository_normalizes_host() {
    let selector: RepositorySelector = "https://GHE.example.test/org/repo".parse().expect("URL");
    assert_eq!(selector.host().as_str(), "ghe.example.test");
    assert_eq!(selector.owner(), "org");
    assert_eq!(selector.name(), "repo");
}

#[test]
fn numbered_pair_selects_default_host_thread() {
    let selector: ThreadSelector = "org/repo#42".parse().expect("numbered pair");
    assert_eq!(selector.number().get(), 42);
    assert_eq!(selector.repository().host().as_str(), "github.com");
}

#[test]
fn pull_url_discards_query_suffix() {
    let selector: ThreadSelector = "https://ghe.example.test/org/repo/pull/7?x=1"
        .parse()
        .expect("pull URL");
    assert_eq!(selector.number().get(), 7);
    assert_eq!(selector.repository().host().as_str(), "ghe.example.test");
}

#[test]
fn repository_url_retains_percent_escape_spelling() {
    let selector: RepositorySelector = "https://github.com/org/repo%20name/?x=1#fragment"
        .parse()
        .expect("literal path");
    assert_eq!(selector.name(), "repo%20name");
    assert_eq!(selector.as_url(), "https://github.com/org/repo%20name");
}

#[test]
fn display_case_remains_part_of_selector_equality() {
    let upper: RepositorySelector = "Example/Project".parse().expect("upper spelling");
    let lower: RepositorySelector = "example/project".parse().expect("lower spelling");
    assert_ne!(upper, lower);
}

#[rstest::rstest]
#[case::missing_owner("repo", ReferenceParseError::InvalidThread)]
#[case::zero_number("org/repo#0", ReferenceParseError::InvalidThread)]
#[case::ambiguous_number("org/repo#1/2", ReferenceParseError::InvalidThread)]
#[case::insecure_url("http://github.com/a/b", ReferenceParseError::InvalidRepository)]
fn rejects_malformed_thread_reference(#[case] value: &str, #[case] expected: ReferenceParseError) {
    let result = value.parse::<ThreadSelector>();
    assert_eq!(result, Err(expected));
}

#[test]
fn repository_pair_rejects_extra_segments() {
    let result = "org/repo/extra".parse::<RepositorySelector>();
    assert_eq!(result, Err(ReferenceParseError::InvalidRepository));
}
