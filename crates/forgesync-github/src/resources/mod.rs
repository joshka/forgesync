//! REST resource families and their page-oriented acquisition results.
//!
//! The public fetch functions in `fetch` request repositories, issue and pull-request discussions,
//! comments, pull-request metadata, and reviews. `normalize` converts provider DTOs into checked
//! `forgesync-core::content` values. [`RestThreadPage`], [`RestCommentPage`], and
//! [`RestReviewPage`] return one page plus continuation context; [`ThreadListState`] selects the
//! enumeration scope.
//!
//! The engine calls these functions while recording durable cursors and per-family outcomes. A
//! page can be valid even when a later page fails. Do not turn a page result into complete
//! collection membership until the engine and store have verified the whole selected page set.
//!
//! Provider response structs remain local to this module. Archive rows, CLI JSON shapes, and
//! domain observations belong to other crates. The REST functions do not write to SQLite or choose
//! a credential.

use std::collections::BTreeMap;

use forgesync_core::content::{
    BranchRef, Comment, Discussion, PullRequestMetadata, Repository, Review, ReviewState,
    ReviewerIdentity, SourceState, ThreadKind,
};
use forgesync_core::identity::{
    CommentId, CommitSha, GitHubHost, ProviderId, RepositoryId, ReviewId, ThreadId, ThreadNumber,
};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::error::GitHubError;
use crate::transport::{GitHubClient, GitHubResponse};

/// One normalized issue-list page and its validated next-page destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestThreadPage {
    /// Issues and pull requests returned by the repository issues endpoint.
    pub discussions: Vec<Discussion>,
    /// Next page URL from the provider, when one remains.
    pub next_page: Option<Url>,
}

/// One normalized issue-comment page and its validated next-page destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestCommentPage {
    /// Discussion comments returned by the issues comments endpoint.
    pub comments: Vec<Comment>,
    /// Next page URL from the provider, when one remains.
    pub next_page: Option<Url>,
}

/// One normalized pull-request review page and its validated next-page destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestReviewPage {
    /// Submitted and pending reviews returned by the pull-request reviews endpoint.
    pub reviews: Vec<Review>,
    /// Next page URL from the provider, when one remains.
    pub next_page: Option<Url>,
}

/// Source states selected from the GitHub issues endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadListState {
    /// Include issues and pull requests in either source state.
    All,
    /// Include only currently open issues and pull requests.
    Open,
    /// Include only currently closed issues and pull requests.
    Closed,
}

mod fetch;
mod normalize;

pub use fetch::{
    fetch_issue_comment_page, fetch_pull_request_metadata, fetch_pull_request_review_page,
    fetch_repository, fetch_thread_page, fetch_thread_page_in_scope, issue_comment_list_url,
    thread_list_url, thread_list_url_in_scope,
};

#[derive(Deserialize)]
struct RestRepository {
    id: u64,
    name: String,
    full_name: Option<String>,
    owner: RestUser,
    default_branch: Option<String>,
    updated_at: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
struct RestIssue {
    id: u64,
    number: u64,
    state: String,
    title: String,
    body: Option<String>,
    created_at: String,
    updated_at: String,
    closed_at: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    labels: Option<Vec<RestLabel>>,
    #[serde(default)]
    assignees: Option<Vec<RestUser>>,
    user: Option<RestUser>,
    pull_request: Option<Value>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct RestComment {
    id: u64,
    #[serde(default)]
    body: String,
    created_at: String,
    updated_at: Option<String>,
    user: Option<Value>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct RestPullRequest {
    base: RestBranchRef,
    head: RestBranchRef,
    draft: bool,
    merged: bool,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
struct RestBranchRef {
    #[serde(rename = "ref")]
    name: String,
    sha: String,
    repo: Option<RestBranchRepository>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
struct RestBranchRepository {
    id: Option<u64>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct RestReview {
    id: u64,
    state: String,
    body: Option<String>,
    submitted_at: Option<String>,
    commit_id: Option<String>,
    user: Option<Value>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
struct RestUser {
    login: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
struct RestLabel {
    name: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[cfg(test)]
mod tests;
