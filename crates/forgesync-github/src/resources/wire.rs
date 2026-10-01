//! REST response shapes before normalization.
//!
//! Optional fields keep the provider's nullable/omitted shape; each normalizer chooses its domain
//! meaning. Flattened `extra` maps retain unmodeled fields as provider data.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize)]
pub struct RestRepository {
    pub id: u64,
    pub name: String,
    pub full_name: Option<String>,
    pub owner: RestUser,
    pub default_branch: Option<String>,
    pub updated_at: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Issues endpoint item; covers both issues and pull requests.
#[derive(Deserialize, Serialize)]
pub struct RestIssue {
    pub id: u64,
    pub number: u64,
    pub state: String,
    pub title: String,
    pub body: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
    pub html_url: Option<String>,
    #[serde(default)]
    pub labels: Option<Vec<RestLabel>>,
    #[serde(default)]
    pub assignees: Option<Vec<RestUser>>,
    pub user: Option<RestUser>,
    /// Presence identifies an issues-endpoint item as a pull request.
    pub pull_request: Option<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
pub struct RestComment {
    pub id: u64,
    #[serde(default)]
    pub body: String,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub user: Option<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
pub struct RestPullRequest {
    pub base: RestBranchRef,
    pub head: RestBranchRef,
    pub draft: bool,
    pub merged: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
pub struct RestBranchRef {
    #[serde(rename = "ref")]
    pub name: String,
    pub sha: String,
    pub repo: Option<RestBranchRepository>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
pub struct RestBranchRepository {
    pub id: Option<u64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
pub struct RestReview {
    pub id: u64,
    pub state: String,
    pub body: Option<String>,
    /// Pending reviews have no submission time.
    pub submitted_at: Option<String>,
    pub commit_id: Option<String>,
    pub user: Option<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
pub struct RestUser {
    pub login: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Serialize)]
pub struct RestLabel {
    pub name: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
