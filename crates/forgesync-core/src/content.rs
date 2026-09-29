//! Normalized GitHub discussion content, independent of transport and storage representation.
//!
//! [`Repository`] identifies the owner and current path. [`Discussion`] is the parent issue or
//! pull request; [`ThreadKind`] and [`SourceState`] describe its source type and state. Comments,
//! pull request metadata, reviews, reviewer identities, and review threads are separate child
//! resources.
//!
//! A provider DTO is converted into these values by `forgesync-github` before an observation
//! reaches the store. The store records each family with its own coverage, so the presence of a
//! `Discussion` does not imply that all comments or reviews were acquired.
//! [`crate::provider_data`] retains extra provider fields without making them archive identity or
//! ordering policy.
//!
//! Use this module when inspecting or constructing normalized content. Use [`crate::observation`]
//! to say when the content was seen, and [`crate::coverage`] to say whether a related collection
//! is complete. Changes to a child type need the corresponding provider normalization and store
//! row conversion reviewed together.

use serde::{Deserialize, Serialize};

use crate::identity::{
    CommentId, CommitSha, ProviderId, RepositoryId, ReviewId, ReviewThreadId, ThreadId,
};
use crate::provider_data::ProviderData;
use crate::timestamp::UtcTimestamp;

/// Whether a discussion is an issue or a pull request.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadKind {
    /// An issue without pull request metadata.
    Issue,
    /// A pull request with selected base and head metadata.
    PullRequest,
}

/// Source-owned open or closed state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    /// The source reports that the discussion is open.
    Open,
    /// The source reports that the discussion is closed.
    Closed,
    /// An unrecognized source state preserved for forward compatibility.
    Other(String),
}

/// A normalized GitHub repository record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Repository {
    /// Stable host-qualified provider identity.
    pub id: RepositoryId,
    /// Current owner name used for display and lookup.
    pub owner: String,
    /// Current repository name used for display and lookup.
    pub name: String,
    /// Current `owner/name` display value.
    pub full_name: String,
    /// Current default branch, when known.
    pub default_branch: Option<String>,
    /// Source update time, when provided and valid.
    pub updated_at: Option<UtcTimestamp>,
    /// Provider fields not yet modeled by Forgesync.
    pub provider_data: ProviderData,
}

/// A normalized issue or pull request discussion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Discussion {
    /// Stable host-qualified provider identity.
    pub id: ThreadId,
    /// Issue or pull request kind.
    pub kind: ThreadKind,
    /// Source-owned open or closed state.
    pub state: SourceState,
    /// Discussion title.
    pub title: String,
    /// Source body; `None` preserves a provider null separately from empty text.
    pub body: Option<String>,
    /// Canonical browser URL, when supplied by the provider.
    pub html_url: Option<String>,
    /// Original source creation time.
    pub created_at: UtcTimestamp,
    /// Original source update time.
    pub updated_at: UtcTimestamp,
    /// Source closure time, when known.
    pub closed_at: Option<UtcTimestamp>,
    /// Current labels in source order.
    pub labels: Vec<String>,
    /// Current assignee logins in source order.
    pub assignees: Vec<String>,
    /// Provider fields not yet modeled by Forgesync.
    pub provider_data: ProviderData,
}

/// Base or head branch context for a pull request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BranchRef {
    /// Branch name reported by the provider.
    pub name: String,
    /// Exact source commit SHA.
    pub sha: CommitSha,
    /// Repository for this branch; a deleted fork may have no head repository.
    pub repository: Option<RepositoryId>,
}

/// Selected pull request metadata needed to interpret review evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PullRequestMetadata {
    /// Base branch and repository context.
    pub base: BranchRef,
    /// Head branch and repository context.
    pub head: BranchRef,
    /// Whether the pull request is a draft.
    pub draft: bool,
    /// Whether GitHub reports the pull request as merged.
    pub merged: bool,
    /// Provider fields not yet modeled by Forgesync, including original base/head values.
    pub provider_data: ProviderData,
}

/// A discussion comment or a comment attached to a review thread.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    /// Stable identity scoped to its parent discussion.
    pub id: CommentId,
    /// Review association for a pull request review comment, when present.
    pub review_id: Option<ReviewId>,
    /// Provider login, when the source still exposes the author.
    pub author: Option<String>,
    /// Comment body.
    pub body: String,
    /// Original creation time.
    pub created_at: UtcTimestamp,
    /// Latest source update time, when exposed.
    pub updated_at: Option<UtcTimestamp>,
    /// Provider fields not yet modeled by Forgesync.
    pub provider_data: ProviderData,
}

/// A pull request review state reported by GitHub.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    /// The review approved the current head.
    Approved,
    /// The review requested changes to the current head.
    ChangesRequested,
    /// The review submitted comments without a vote.
    Commented,
    /// The source dismissed the review.
    Dismissed,
    /// The review is pending submission.
    Pending,
    /// An unrecognized provider state preserved for forward compatibility.
    Other(String),
}

/// A submitted pull request review.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Review {
    /// Stable identity scoped to its pull request.
    pub id: ReviewId,
    /// Provider review state.
    pub state: ReviewState,
    /// Reviewer identity as reported by the provider, when still available.
    pub reviewer: Option<ReviewerIdentity>,
    /// Review body, preserving an empty body separately from no body.
    pub body: Option<String>,
    /// Source submission time, when present.
    pub submitted_at: Option<UtcTimestamp>,
    /// Commit SHA reviewed by this submission, when present.
    pub commit_sha: Option<CommitSha>,
    /// Provider fields not yet modeled by Forgesync.
    pub provider_data: ProviderData,
}

/// Provider identity and source provenance for a pull-request reviewer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReviewerIdentity {
    /// Stable provider user ID, when supplied.
    pub provider_id: Option<ProviderId>,
    /// Current provider login, when supplied.
    pub login: Option<String>,
    /// Original user object, including fields not yet modeled by Forgesync.
    pub provider_data: ProviderData,
}

/// Current review-thread state and comments from a pull request review.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReviewThread {
    /// Stable identity scoped to its pull request.
    pub id: ReviewThreadId,
    /// Pull request head SHA against which resolution and outdated state were observed.
    pub head_sha: CommitSha,
    /// Whether the source reports the thread resolved.
    pub is_resolved: bool,
    /// Whether the reviewed line is outdated against the current head.
    pub is_outdated: bool,
    /// Reviewed path, when still available.
    pub path: Option<String>,
    /// Current line number, when available.
    pub line: Option<u64>,
    /// Current comments associated with this review thread.
    pub comments: Vec<Comment>,
    /// Provider fields not yet modeled by Forgesync.
    pub provider_data: ProviderData,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::content::ReviewState;
    use crate::provider_data::ProviderData;

    #[test]
    fn unknown_provider_fields_survive_a_json_round_trip() {
        let data = ProviderData::from_value(json!({
            "new_graphql_flag": true,
            "nested": { "future_counter": 9007199254740997_u64, "values": [null, "kept"] }
        }))
        .expect("provider object");

        let encoded = serde_json::to_value(&data).expect("serialize provider data");
        let decoded: ProviderData = serde_json::from_value(encoded.clone()).expect("deserialize");
        assert_eq!(decoded, data);
        assert_eq!(
            encoded["nested"]["future_counter"],
            json!(9007199254740997_u64)
        );
        assert_eq!(encoded["new_graphql_flag"], json!(true));
    }

    #[test]
    fn unknown_review_states_remain_representable() {
        let state = ReviewState::Other("FUTURE_STATE".to_owned());
        let decoded: ReviewState =
            serde_json::from_value(serde_json::to_value(&state).expect("serialize state"))
                .expect("deserialize state");
        assert_eq!(decoded, state);
    }
}
