//! # Successful response decoding
//!
//! These local cases isolate the conversion after status, body-size, and origin checks.
//! A valid payload keeps its trusted pagination destination. Invalid syntax and mismatched
//! DTO shape share the terminal `InvalidJson` category without exposing provider body text.
//!
//! The byte inputs are deliberately small and explicit; HTTP retry and origin validation
//! remain covered by the client transport scenarios. These tests perform no network I/O.
//! `ResponseBody` is an implementation owner inside the private response module, so these
//! cases do not create a new public transport construction API.

use reqwest::Url;

use crate::error::GitHubError;
use crate::transport::response::ResponseBody;

#[test]
fn decoded_page_retains_its_pagination_destination() {
    let next = Url::parse("https://github.example/api/v3/items?page=2").expect("next URL");
    let response = ResponseBody {
        body: br#"[1, 2]"#.to_vec(),
        next_page: Some(next.clone()),
    };

    let page = response.decode::<Vec<u64>>().expect("typed page");

    assert_eq!(page.value, vec![1, 2]);
    assert_eq!(page.next_page, Some(next));
}

#[test]
fn malformed_success_body_returns_invalid_json() {
    let response = ResponseBody {
        body: b"not JSON".to_vec(),
        next_page: None,
    };

    let error = response.decode::<Vec<u64>>().expect_err("invalid page");

    assert!(matches!(error, GitHubError::InvalidJson));
}

#[test]
fn mismatched_success_dto_returns_invalid_json() {
    let response = ResponseBody {
        body: br#"{"id": 1}"#.to_vec(),
        next_page: None,
    };

    let error = response.decode::<Vec<u64>>().expect_err("invalid page");

    assert!(matches!(error, GitHubError::InvalidJson));
}
