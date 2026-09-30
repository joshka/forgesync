//! Environment precedence and bounded credential subprocesses.
//!
//! Direct values establish selection without modifying the process environment. The Unix-only
//! subprocess cases use a sleeping child to observe timeout and cancellation independently of
//! GitHub CLI installation or network access. These tests prove lookup behavior, not credential
//! validity at the provider. Synthetic token strings remain visible only in test expectations.

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::credentials::{
    CredentialError, choose_environment_token, run_credential_process,
    valid_environment_variable_name,
};

#[test]
fn configured_environment_token_precedes_github_token() {
    let selected = choose_environment_token(
        Some(OsString::from("configured-token")),
        Some(OsString::from("github-token")),
    )
    .expect("valid tokens");

    assert_eq!(selected.expect("token").expose(), "configured-token");
}

#[test]
fn empty_configured_environment_token_falls_back_to_github_token() {
    let selected = choose_environment_token(
        Some(OsString::from("  ")),
        Some(OsString::from("github-token")),
    )
    .expect("valid token");

    assert_eq!(selected.expect("token").expose(), "github-token");
}

#[rstest::rstest]
#[case::portable_name("FORGESYNC_GITHUB_TOKEN", true)]
#[case::leading_digit("9TOKEN", false)]
#[case::shell_punctuation("TOKEN;echo", false)]
fn environment_variable_name_has_portable_spelling(#[case] name: &str, #[case] valid: bool) {
    assert_eq!(valid_environment_variable_name(name), valid);
}

#[cfg(unix)]
#[tokio::test]
async fn credential_subprocess_timeout_is_bounded() {
    let result = run_credential_process(
        Path::new("/bin/sleep"),
        &[OsString::from("5")],
        Duration::from_millis(30),
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, Err(CredentialError::TimedOut));
}

#[cfg(unix)]
#[tokio::test]
async fn credential_subprocess_cancellation_is_bounded() {
    let cancellation = CancellationToken::new();
    let child_cancellation = cancellation.clone();
    let task = tokio::spawn(async move {
        run_credential_process(
            Path::new("/bin/sleep"),
            &[OsString::from("5")],
            Duration::from_secs(10),
            &child_cancellation,
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    cancellation.cancel();

    assert_eq!(
        task.await.expect("credential task"),
        Err(CredentialError::Cancelled)
    );
}
