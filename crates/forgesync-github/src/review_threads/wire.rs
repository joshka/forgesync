//! # Keep GraphQL query variables and response shapes distinct from domain evidence
//!
//! These private wire values describe the outer review-thread query and nested comment query.
//! Acquisition encodes variables, checks envelope errors, and traverses each connection's page
//! metadata. `review_threads::normalize` converts fully acquired nodes into checked core content.
//!
//! Nullable repository, node, connection, member, and page-info fields preserve provider omission
//! rather than silently treating it as an empty complete collection. Acquisition decides which
//! values are required. Resolution and outdated booleans remain source facts, not control switches.
//!
//! Flattened fields retain source extensions. Public fields allow sibling acquisition and
//! normalization to share these shapes inside a private module; no wire type is exported as an
//! application API, archive row, or normalized discussion. Serde names remain provider spellings.

use std::collections::BTreeMap;

use forgesync_core::identity::ProviderId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Operation envelope retaining nullable data and provider errors for acquisition validation.
#[derive(Deserialize)]
pub struct GraphqlEnvelope<T> {
    /// Nullable decoded operation data; acquisition rejects absent required data.
    pub data: Option<T>,
    /// Provider error entries; acquisition rejects partial-error envelopes before claiming
    /// completeness.
    #[serde(default)]
    pub errors: Vec<Value>,
}

/// Outer-page query coordinates for one repository-scoped pull request.
#[derive(Serialize)]
pub struct ReviewThreadsVariables<'a> {
    /// Selected repository owner used only for this outer-page query.
    pub owner: &'a str,
    /// Selected repository name used with the supplied owner.
    pub repo: &'a str,
    /// Positive pull-request number supplied by the checked caller scope.
    pub number: u64,
    /// Optional opaque continuation token for the selected connection.
    pub cursor: Option<&'a str>,
}

/// Nested-page query coordinates for one checked review-thread node.
#[derive(Serialize)]
pub struct ReviewThreadCommentsVariables<'a> {
    /// Checked GraphQL review-thread node ID for the nested comment query.
    #[serde(rename = "threadID")]
    pub thread_id: &'a ProviderId,
    /// Optional opaque continuation token for the selected connection.
    pub cursor: Option<&'a str>,
}

/// Outer query data before validating the selected repository and pull request.
#[derive(Deserialize)]
pub struct ReviewThreadsData {
    /// Nullable repository node; absence is distinct from an empty thread connection.
    pub repository: Option<GraphqlRepository>,
}

/// Repository selection response with a nullable pull-request node.
#[derive(Deserialize)]
pub struct GraphqlRepository {
    /// Nullable pull-request node required by the outer acquisition operation.
    #[serde(rename = "pullRequest")]
    pub pull_request: Option<GraphqlPullRequest>,
}

/// Pull-request response containing the independently paged review-thread connection.
#[derive(Deserialize)]
pub struct GraphqlPullRequest {
    /// Nullable outer review-thread connection with independent pagination.
    #[serde(rename = "reviewThreads")]
    pub review_threads: Option<GraphqlConnection<GraphqlReviewThread>>,
}

/// Generic node page with nullable membership and continuation metadata.
#[derive(Deserialize)]
pub struct GraphqlConnection<T> {
    /// Nullable page members; acquisition validates presence before normalization.
    pub nodes: Option<Vec<T>>,
    /// Nullable continuation metadata; required for truthful connection completion.
    #[serde(rename = "pageInfo")]
    pub page_info: Option<GraphqlPageInfo>,
}

/// Provider continuation claim and cursor, validated together before further acquisition.
#[derive(Deserialize)]
pub struct GraphqlPageInfo {
    /// Provider continuation claim, validated alongside the cursor.
    #[serde(rename = "hasNextPage")]
    pub has_next_page: Option<bool>,
    /// Opaque cursor required when the provider claims another page.
    #[serde(rename = "endCursor")]
    pub end_cursor: Option<String>,
}

/// Raw thread node before required state and fully paged comments are normalized.
#[derive(Deserialize)]
pub struct GraphqlReviewThread {
    /// Raw provider node identity checked during acquisition or normalization.
    pub id: String,
    /// Optional provider resolution fact; normalization requires an explicit value.
    #[serde(rename = "isResolved")]
    pub is_resolved: Option<bool>,
    /// Optional provider location-freshness fact, independent of resolution.
    #[serde(rename = "isOutdated")]
    pub is_outdated: Option<bool>,
    /// Optional source file context retained without synthesizing a location.
    pub path: Option<String>,
    /// Optional signed provider line checked for domain numeric representability.
    pub line: Option<i64>,
    /// Optional provider range context retained in extension data.
    #[serde(rename = "startLine")]
    pub start_line: Option<i64>,
    /// Nullable nested comment connection; acquisition completes its pages before normalization.
    pub comments: Option<GraphqlConnection<GraphqlComment>>,
    /// Unmodeled fields retained as provider extensions rather than workflow policy.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Raw review comment retaining author, file, timestamp, and containing-review context.
#[derive(Deserialize)]
pub struct GraphqlComment {
    /// Raw provider node identity checked during acquisition or normalization.
    pub id: String,
    /// Optional numeric REST identity retained alongside the GraphQL node ID.
    #[serde(rename = "databaseId")]
    pub database_id: Option<u64>,
    /// Required source comment text preserved by normalization.
    pub body: String,
    /// Nullable author data, separate from the comment provider identity.
    pub author: Option<GraphqlAuthor>,
    /// Optional source file context retained without synthesizing a location.
    pub path: Option<String>,
    /// Optional source diff context retained as provider data.
    #[serde(rename = "diffHunk")]
    pub diff_hunk: Option<String>,
    /// Required raw source creation timestamp parsed by normalization.
    #[serde(rename = "createdAt")]
    pub created_at: String,
    /// Optional raw update timestamp with fallback owned by normalization.
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<String>,
    /// Optional provider browser URL retained as source metadata.
    pub url: Option<String>,
    /// Optional containing review identity preserved with the comment.
    #[serde(rename = "pullRequestReview")]
    pub pull_request_review: Option<GraphqlReviewRef>,
    /// Unmodeled fields retained as provider extensions rather than workflow policy.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Nullable author identity and extensions, distinct from comment identity.
#[derive(Deserialize)]
pub struct GraphqlAuthor {
    /// Nullable author login; an absent login is not a fabricated user.
    pub login: Option<String>,
    /// Unmodeled fields retained as provider extensions rather than workflow policy.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Containing review node identity retained with a comment.
#[derive(Deserialize)]
pub struct GraphqlReviewRef {
    /// Raw provider node identity checked during acquisition or normalization.
    pub id: String,
}

/// Nested query result before validating the selected review-thread node.
#[derive(Deserialize)]
pub struct ReviewThreadCommentsData {
    /// Nullable review-thread node returned by nested comment acquisition.
    pub node: Option<GraphqlReviewThreadNode>,
}

/// Nested node response containing the current comment connection page.
#[derive(Deserialize)]
pub struct GraphqlReviewThreadNode {
    /// Nullable nested comment connection; acquisition completes its pages before normalization.
    pub comments: Option<GraphqlConnection<GraphqlComment>>,
}
