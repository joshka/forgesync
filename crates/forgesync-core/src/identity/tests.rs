use serde_json::json;

use super::{CommitSha, GitHubHost, IdentityError, ProviderId, RepositoryId, RunId, ThreadNumber};

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
