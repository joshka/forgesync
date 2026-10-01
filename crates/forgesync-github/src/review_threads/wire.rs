//! GraphQL variables and response shapes for the review-thread queries.
//!
//! Fields the GitHub schema declares non-null are non-optional, so a payload missing them fails
//! decoding (`InvalidJson`) instead of being read as an empty, complete collection.

use std::collections::BTreeMap;

use forgesync_core::identity::ProviderId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize)]
pub struct GraphqlEnvelope<T> {
    pub data: Option<T>,
    #[serde(default)]
    pub errors: Vec<Value>,
}

#[derive(Serialize)]
pub struct ReviewThreadsVariables<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub number: u64,
    pub cursor: Option<&'a str>,
}

#[derive(Serialize)]
pub struct ReviewThreadCommentsVariables<'a> {
    #[serde(rename = "threadID")]
    pub thread_id: &'a ProviderId,
    pub cursor: Option<&'a str>,
}

#[derive(Deserialize)]
pub struct ReviewThreadsData {
    pub repository: Option<GraphqlRepository>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlRepository {
    pub pull_request: Option<GraphqlPullRequest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlPullRequest {
    pub review_threads: GraphqlConnection<GraphqlReviewThread>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlConnection<T> {
    pub nodes: Vec<T>,
    pub page_info: GraphqlPageInfo,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlPageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlReviewThread {
    pub id: String,
    pub is_resolved: bool,
    pub is_outdated: bool,
    pub path: Option<String>,
    pub line: Option<i64>,
    pub start_line: Option<i64>,
    pub comments: GraphqlConnection<GraphqlComment>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlComment {
    pub id: String,
    pub database_id: Option<u64>,
    pub body: String,
    pub author: Option<GraphqlAuthor>,
    pub path: Option<String>,
    pub diff_hunk: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub url: Option<String>,
    pub pull_request_review: Option<GraphqlReviewRef>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
pub struct GraphqlAuthor {
    pub login: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
pub struct GraphqlReviewRef {
    pub id: String,
}

#[derive(Deserialize)]
pub struct ReviewThreadCommentsData {
    pub node: Option<GraphqlReviewThreadNode>,
}

#[derive(Deserialize)]
pub struct GraphqlReviewThreadNode {
    pub comments: GraphqlConnection<GraphqlComment>,
}
