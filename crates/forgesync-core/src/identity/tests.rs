//! Identity normalization and rejection at the construction and deserialization boundary.

use serde_json::json;

use crate::identity::{
    CommitSha, GitHubHost, IdentityError, ProviderId, RepositoryId, RunId, ThreadNumber,
};

#[rstest::rstest]
#[case::enterprise(
    "HTTPS://GHE.Example.Test:8443/",
    "ghe.example.test:8443",
    "https://ghe.example.test:8443"
)]
#[case::ipv6("https://[2001:DB8::1]:443", "[2001:db8::1]", "https://[2001:db8::1]")]
#[case::default_port("github.com:443", "github.com", "https://github.com")]
fn host_identity_normalizes_authority(
    #[case] input: &str,
    #[case] authority: &str,
    #[case] origin: &str,
) {
    let host = GitHubHost::parse(input).expect("valid authority");
    assert_eq!(host.as_str(), authority);
    assert_eq!(host.https_origin(), origin);
}

#[rstest::rstest]
#[case::insecure_scheme("http://github.com")]
#[case::api_path("https://github.com/api/v3")]
#[case::credentials("https://user:pass@github.com")]
#[case::empty_dns_label("github..com")]
#[case::whitespace("bad host")]
#[case::zero_port("github.com:0")]
#[case::invalid_ipv6("[not-ipv6]")]
fn host_identity_rejects_invalid_authority(#[case] value: &str) {
    assert_eq!(
        GitHubHost::parse(value),
        Err(IdentityError::InvalidGitHubHost)
    );
}

#[rstest::rstest]
#[case::whitespace(" ")]
#[case::line_break("bad\nid")]
fn provider_identity_rejects_invalid_text(#[case] input: &str) {
    assert_eq!(
        ProviderId::new(input),
        Err(IdentityError::InvalidProviderId)
    );
}

#[test]
fn zero_thread_number_is_rejected() {
    assert_eq!(ThreadNumber::new(0), Err(IdentityError::NotPositive));
}

#[test]
fn zero_run_id_is_rejected() {
    assert_eq!(RunId::new(0), Err(IdentityError::NotPositive));
}

#[test]
fn abbreviated_commit_sha_is_rejected() {
    assert_eq!(
        CommitSha::new("deadbeef"),
        Err(IdentityError::InvalidCommitSha)
    );
}

#[test]
fn full_commit_sha_normalizes_hexadecimal_case() {
    let sha = CommitSha::new("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").expect("full SHA-1");
    assert_eq!(sha.as_str(), "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
}

#[test]
fn repository_identity_round_trips_through_json() {
    let host = GitHubHost::parse("github.com").expect("host");
    let provider_id = ProviderId::new("R_fixture_41").expect("provider ID");
    let id = RepositoryId::new(host, provider_id);
    let value = serde_json::to_value(&id).expect("serialize repository ID");
    let decoded: RepositoryId = serde_json::from_value(value).expect("deserialize ID");
    assert_eq!(decoded, id);
}

#[test]
fn host_deserialization_enforces_authority_validation() {
    let result = serde_json::from_value::<GitHubHost>(json!("http://github.com"));
    let error = result.expect_err("reject insecure serialized host");
    assert_eq!(error.classify(), serde_json::error::Category::Data);
    assert!(
        error
            .to_string()
            .contains(&IdentityError::InvalidGitHubHost.to_string())
    );
}

#[test]
fn identity_deserialization_rejects_invalid_values() {
    assert!(serde_json::from_value::<RunId>(json!(0)).is_err());
    assert!(serde_json::from_value::<ProviderId>(json!("bad id")).is_err());
    let sha: CommitSha = serde_json::from_value(json!("A".repeat(40))).expect("full SHA-1");
    assert_eq!(serde_json::to_value(sha).unwrap(), json!("a".repeat(40)));
}
