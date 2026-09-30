//! # Retain REST response shape before domain normalization
//!
//! These private-module DTOs describe repository, discussion, comment, pull-request, and review
//! endpoint payloads. Acquisition decodes them; `resources::normalize` checks identities and
//! timestamps and produces core content. Public page results remain in `resources`, separate from
//! these raw provider fields.
//!
//! Optional fields preserve the provider's nullable/omitted shape until each normalizer chooses its
//! domain meaning. Comment body alone defaults to empty when absent. Flattened extension maps
//! retain unknown fields; their presence does not establish identity, collection completeness, or
//! ordering.
//!
//! Structs and fields are public only inside this private module so sibling acquisition and
//! normalization can share one response definition. They are not exported as application APIs or
//! archive/CLI representations. Keep Serde names and defaults aligned with the endpoint contract.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Repository endpoint response before owner, timestamp, and provider identity validation.
#[derive(Deserialize)]
pub struct RestRepository {
    /// Raw numeric provider ID; normalization constructs the scoped checked identity.
    pub id: u64,
    /// Provider-supplied display name retained before domain projection.
    pub name: String,
    /// Optional owner/name display path; repository normalization supplies its fallback.
    pub full_name: Option<String>,
    /// Nested owner response whose login is required for repository normalization.
    pub owner: RestUser,
    /// Optional source default branch, independent of commit revision validation.
    pub default_branch: Option<String>,
    /// Raw update timestamp; interpretation and any source fallback belong to normalization.
    pub updated_at: Option<String>,
    /// Unmodeled provider fields retained without turning them into workflow policy.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Issues endpoint item, covering both issues and pull requests before kind normalization.
#[derive(Deserialize, Serialize)]
pub struct RestIssue {
    /// Raw numeric provider ID; normalization constructs the scoped checked identity.
    pub id: u64,
    /// Raw repository-local number checked for positivity during normalization.
    pub number: u64,
    /// Source spelling retained so unknown states can remain explicit.
    pub state: String,
    /// Source discussion title without presentation or search normalization.
    pub title: String,
    /// Source body representation, with this response type’s null/default behavior preserved.
    pub body: Option<String>,
    /// Required raw creation timestamp parsed before constructing domain content.
    pub created_at: String,
    /// Raw update timestamp; interpretation and any source fallback belong to normalization.
    pub updated_at: String,
    /// Optional raw closure timestamp, distinct from the current source-state spelling.
    pub closed_at: Option<String>,
    /// Optional provider browser URL retained as source metadata.
    pub html_url: Option<String>,
    /// Optional label collection; absence does not establish independent acquisition coverage.
    #[serde(default)]
    pub labels: Option<Vec<RestLabel>>,
    /// Optional nested assignee metadata retained with the source discussion.
    #[serde(default)]
    pub assignees: Option<Vec<RestUser>>,
    /// Optional author/reviewer source data, interpreted separately from resource identity.
    pub user: Option<RestUser>,
    /// Presence identifies an issues-endpoint item as a pull request.
    pub pull_request: Option<Value>,
    /// Unmodeled provider fields retained without turning them into workflow policy.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Issue-comment response with raw timestamps and nullable author information.
#[derive(Deserialize)]
pub struct RestComment {
    /// Raw numeric provider ID; normalization constructs the scoped checked identity.
    pub id: u64,
    /// Source body representation, with this response type’s null/default behavior preserved.
    #[serde(default)]
    pub body: String,
    /// Required raw creation timestamp parsed before constructing domain content.
    pub created_at: String,
    /// Raw update timestamp; interpretation and any source fallback belong to normalization.
    pub updated_at: Option<String>,
    /// Optional author/reviewer source data, interpreted separately from resource identity.
    pub user: Option<Value>,
    /// Unmodeled provider fields retained without turning them into workflow policy.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Pull-request endpoint metadata before branch revision and repository conversion.
#[derive(Deserialize)]
pub struct RestPullRequest {
    /// Raw base branch context to normalize with the selected pull request.
    pub base: RestBranchRef,
    /// Raw head branch context used for checked revision-bound review evidence.
    pub head: RestBranchRef,
    /// Required provider draft fact, separate from local acquisition or triage state.
    pub draft: bool,
    /// Required provider merge fact, separate from discussion open/closed spelling.
    pub merged: bool,
    /// Unmodeled provider fields retained without turning them into workflow policy.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Provider branch context, distinct from a checked commit revision.
#[derive(Deserialize, Serialize)]
pub struct RestBranchRef {
    /// Provider-supplied display name retained before domain projection.
    #[serde(rename = "ref")]
    pub name: String,
    /// Full raw commit spelling validated and normalized by the core commit-ID constructor.
    pub sha: String,
    /// Optional branch repository response; missing repositories remain explicit.
    pub repo: Option<RestBranchRepository>,
    /// Unmodeled provider fields retained without turning them into workflow policy.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Optional branch repository metadata, including deleted or unavailable repository context.
#[derive(Deserialize, Serialize)]
pub struct RestBranchRepository {
    /// Raw numeric provider ID; normalization constructs the scoped checked identity.
    pub id: Option<u64>,
    /// Unmodeled provider fields retained without turning them into workflow policy.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Review endpoint item retaining pending-review nulls and unknown source states.
#[derive(Deserialize)]
pub struct RestReview {
    /// Raw numeric provider ID; normalization constructs the scoped checked identity.
    pub id: u64,
    /// Source spelling retained so unknown states can remain explicit.
    pub state: String,
    /// Source body representation, with this response type’s null/default behavior preserved.
    pub body: Option<String>,
    /// Optional raw submission time; pending reviews may have no submitted timestamp.
    pub submitted_at: Option<String>,
    /// Optional raw revision associated with the review, validated when present.
    pub commit_id: Option<String>,
    /// Optional author/reviewer source data, interpreted separately from resource identity.
    pub user: Option<Value>,
    /// Unmodeled provider fields retained without turning them into workflow policy.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Nested user data retained for owner or reviewer normalization, rather than a domain identity.
#[derive(Deserialize, Serialize)]
pub struct RestUser {
    /// Optional provider login; required or absent according to the owning normalization context.
    pub login: Option<String>,
    /// Unmodeled provider fields retained without turning them into workflow policy.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Label name and retained provider extensions before discussion label projection.
#[derive(Deserialize, Serialize)]
pub struct RestLabel {
    /// Provider-supplied display name retained before domain projection.
    pub name: String,
    /// Unmodeled provider fields retained without turning them into workflow policy.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
