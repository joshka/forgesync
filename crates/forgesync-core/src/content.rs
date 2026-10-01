//! Normalized GitHub discussion content, independent of transport and storage representation.
//!
//! Each child family has its own coverage, so the presence of a `Discussion` does not imply that
//! all comments or reviews were acquired. Construction does not prove that a child belongs to its
//! supplied parent; provider normalization and archive application enforce those relationships.
//! Keep null-versus-empty source data where the field contract distinguishes it.

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
    Issue,
    PullRequest,
}

impl ThreadKind {
    /// Returns the serde and archive spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::PullRequest => "pull_request",
        }
    }
}

/// Source-owned open or closed state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    Open,
    Closed,
    /// An unrecognized source state preserved for forward compatibility.
    Other(String),
}

impl SourceState {
    /// Returns `open`, `closed`, or the unrecognized provider state unchanged.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Other(value) => value,
        }
    }
}

/// A normalized GitHub repository record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Repository {
    pub id: RepositoryId,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub default_branch: Option<String>,
    pub updated_at: Option<UtcTimestamp>,
    pub provider_data: ProviderData,
}

/// A normalized issue or pull request discussion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Discussion {
    pub id: ThreadId,
    pub kind: ThreadKind,
    pub state: SourceState,
    pub title: String,
    /// Source body; `None` preserves a provider null separately from empty text.
    pub body: Option<String>,
    pub html_url: Option<String>,
    pub created_at: UtcTimestamp,
    pub updated_at: UtcTimestamp,
    pub closed_at: Option<UtcTimestamp>,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
    pub provider_data: ProviderData,
}

/// Base or head branch context for a pull request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BranchRef {
    pub name: String,
    pub sha: CommitSha,
    /// Repository for this branch; a deleted fork may have no head repository.
    pub repository: Option<RepositoryId>,
}

/// Selected pull request metadata needed to interpret review evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PullRequestMetadata {
    pub base: BranchRef,
    pub head: BranchRef,
    pub draft: bool,
    pub merged: bool,
    /// Provider fields not yet modeled by Forgesync, including original base/head values.
    pub provider_data: ProviderData,
}

/// A discussion comment or a comment attached to a review thread.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub id: CommentId,
    /// Review association for a pull request review comment, when present.
    pub review_id: Option<ReviewId>,
    /// Provider login, when the source still exposes the author.
    pub author: Option<String>,
    pub body: String,
    pub created_at: UtcTimestamp,
    pub updated_at: Option<UtcTimestamp>,
    pub provider_data: ProviderData,
}

/// A pull request review state reported by GitHub.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Pending,
    /// An unrecognized provider state preserved for forward compatibility.
    Other(String),
}

/// A submitted pull request review.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Review {
    pub id: ReviewId,
    pub state: ReviewState,
    pub reviewer: Option<ReviewerIdentity>,
    /// Review body, preserving an empty body separately from no body.
    pub body: Option<String>,
    pub submitted_at: Option<UtcTimestamp>,
    pub commit_sha: Option<CommitSha>,
    pub provider_data: ProviderData,
}

/// Provider identity and source provenance for a pull-request reviewer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReviewerIdentity {
    pub provider_id: Option<ProviderId>,
    pub login: Option<String>,
    /// Original user object, including fields not yet modeled by Forgesync.
    pub provider_data: ProviderData,
}

/// Current review-thread state and comments from a pull request review.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReviewThread {
    pub id: ReviewThreadId,
    /// Pull request head SHA against which resolution and outdated state were observed.
    pub head_sha: CommitSha,
    pub is_resolved: bool,
    /// Whether the reviewed line is outdated against the current head.
    pub is_outdated: bool,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub comments: Vec<Comment>,
    pub provider_data: ProviderData,
}

#[cfg(test)]
mod tests {
    //! Retained provider fields and unknown review states survive a JSON round trip.

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
        assert_eq!(
            encoded["nested"]["future_counter"],
            json!(9007199254740997_u64)
        );
        assert_eq!(encoded["new_graphql_flag"], json!(true));
        let decoded: ProviderData = serde_json::from_value(encoded).expect("deserialize");
        assert_eq!(decoded, data);
    }

    #[test]
    fn unknown_review_states_remain_representable() {
        let state = ReviewState::Other("FUTURE_STATE".to_owned());
        let encoded = serde_json::to_value(&state).expect("serialize state");
        let decoded: ReviewState = serde_json::from_value(encoded).expect("deserialize state");
        assert_eq!(decoded, state);
    }
}
