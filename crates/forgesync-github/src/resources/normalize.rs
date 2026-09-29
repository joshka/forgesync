//! Convert REST response shapes into checked domain content.
//!
//! Repository and discussion normalization establish provider identity, current paths, source
//! state, and timestamps. Child normalizers attach comments, reviews, reviewer identities, and
//! pull-request head/base metadata to their checked parent thread. Unknown review states remain
//! explicit rather than being guessed into approval or dismissal.
//!
//! Normalization happens before an observation reaches the store. It validates required provider
//! fields and retains unmapped fields in `ProviderData`, but it does not decide source ordering,
//! collection completeness, or canonical archive membership. A JSON decoding failure becomes a
//! typed provider error without exposing a raw response body.
//!
//! When GitHub adds a field, decide whether it changes a domain invariant or is merely retained
//! provider data. A new resource family also needs engine acquisition and store coverage handling;
//! adding a DTO alone does not make the archive complete.

use super::{
    BTreeMap, BranchRef, Comment, CommentId, CommitSha, Discussion, GitHubError, GitHubHost,
    ProviderData, ProviderId, PullRequestMetadata, Repository, RepositoryId, RestBranchRef,
    RestComment, RestIssue, RestPullRequest, RestRepository, RestReview, Review, ReviewId,
    ReviewState, ReviewerIdentity, SourceState, ThreadId, ThreadKind, ThreadNumber, UtcTimestamp,
    Value,
};

/// Converts a provider repository response to checked domain identity.
pub fn normalize_repository(
    host: &GitHubHost,
    repository: RestRepository,
) -> Result<Repository, GitHubError> {
    let owner = repository
        .owner
        .login
        .as_deref()
        .filter(|login| !login.is_empty())
        .ok_or(GitHubError::InvalidProviderData)?;
    let full_name = repository
        .full_name
        .unwrap_or_else(|| format!("{owner}/{}", repository.name));
    let mut provider_data = provider_data(repository.extra);
    provider_data.insert(
        "owner",
        serde_json::to_value(&repository.owner).map_err(json_error)?,
    );
    let id = RepositoryId::new(
        host.clone(),
        ProviderId::new(repository.id.to_string()).map_err(|_| GitHubError::InvalidProviderData)?,
    );
    Ok(Repository {
        id,
        owner: owner.to_owned(),
        name: repository.name,
        full_name,
        default_branch: repository.default_branch,
        updated_at: repository.updated_at.map(parse_timestamp).transpose()?,
        provider_data,
    })
}

/// Converts an issue or pull-request issue record to normalized discussion content.
pub fn normalize_issue(
    repository: &Repository,
    issue: RestIssue,
) -> Result<Discussion, GitHubError> {
    let id = ThreadId::new(
        repository.id.clone(),
        ProviderId::new(issue.id.to_string()).map_err(|_| GitHubError::InvalidProviderData)?,
        ThreadNumber::new(issue.number).map_err(|_| GitHubError::InvalidProviderData)?,
    );
    let source_state = match issue.state.as_str() {
        "open" => SourceState::Open,
        "closed" => SourceState::Closed,
        state => SourceState::Other(state.to_owned()),
    };
    let labels = issue
        .labels
        .iter()
        .flatten()
        .map(|label| label.name.clone())
        .collect::<Vec<_>>();
    let assignees = issue
        .assignees
        .iter()
        .flatten()
        .filter_map(|assignee| assignee.login.clone())
        .collect::<Vec<_>>();
    let mut provider_data = provider_data(issue.extra);
    if let Some(user) = issue.user {
        provider_data.insert("user", serde_json::to_value(user).map_err(json_error)?);
    }
    if let Some(labels) = issue.labels {
        provider_data.insert(
            "labels_source",
            serde_json::to_value(labels).map_err(json_error)?,
        );
    }
    if let Some(assignees) = issue.assignees {
        provider_data.insert(
            "assignees_source",
            serde_json::to_value(assignees).map_err(json_error)?,
        );
    }
    if let Some(pull_request) = issue.pull_request {
        provider_data.insert("pull_request", pull_request);
    }

    Ok(Discussion {
        id,
        kind: if provider_data.get("pull_request").is_some() {
            ThreadKind::PullRequest
        } else {
            ThreadKind::Issue
        },
        state: source_state,
        title: issue.title,
        body: issue.body,
        html_url: issue.html_url,
        created_at: parse_timestamp(issue.created_at)?,
        updated_at: parse_timestamp(issue.updated_at)?,
        closed_at: issue.closed_at.map(parse_timestamp).transpose()?,
        labels,
        assignees,
        provider_data,
    })
}

/// Converts a REST comment while preserving source fields needed by the archive.
pub fn normalize_comment(thread: &ThreadId, comment: RestComment) -> Result<Comment, GitHubError> {
    let provider_id =
        ProviderId::new(comment.id.to_string()).map_err(|_| GitHubError::InvalidProviderData)?;
    let created_at =
        UtcTimestamp::parse(&comment.created_at).map_err(|_| GitHubError::InvalidProviderData)?;
    let updated_at = comment
        .updated_at
        .map(|value| UtcTimestamp::parse(&value).map_err(|_| GitHubError::InvalidProviderData))
        .transpose()?;
    let author = comment
        .user
        .as_ref()
        .and_then(|user| user.get("login"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut provider_data = provider_data(comment.extra);
    if let Some(user) = comment.user {
        provider_data.insert("user", user);
    }
    Ok(Comment {
        id: CommentId::new(thread.clone(), provider_id),
        review_id: None,
        author,
        body: comment.body,
        created_at,
        updated_at,
        provider_data,
    })
}

/// Converts head and base metadata into normalized pull-request evidence.
pub fn normalize_pull_request(
    repository: &Repository,
    pull_request: RestPullRequest,
) -> Result<PullRequestMetadata, GitHubError> {
    let base_source = serde_json::to_value(&pull_request.base).map_err(json_error)?;
    let head_source = serde_json::to_value(&pull_request.head).map_err(json_error)?;
    let base = normalize_branch_ref(repository.id.host(), pull_request.base)?;
    let head = normalize_branch_ref(repository.id.host(), pull_request.head)?;
    let mut provider_data = provider_data(pull_request.extra);
    provider_data.insert("base_source", base_source);
    provider_data.insert("head_source", head_source);

    Ok(PullRequestMetadata {
        base,
        head,
        draft: pull_request.draft,
        merged: pull_request.merged,
        provider_data,
    })
}

/// Validates a provider branch reference before it enters domain content.
pub fn normalize_branch_ref(
    host: &GitHubHost,
    branch: RestBranchRef,
) -> Result<BranchRef, GitHubError> {
    let repository = branch
        .repo
        .and_then(|repository| repository.id)
        .map(|id| {
            let provider_id =
                ProviderId::new(id.to_string()).map_err(|_| GitHubError::InvalidProviderData)?;
            Ok::<_, GitHubError>(RepositoryId::new(host.clone(), provider_id))
        })
        .transpose()?;
    let sha = CommitSha::new(branch.sha).map_err(|_| GitHubError::InvalidProviderData)?;
    Ok(BranchRef {
        name: branch.name,
        sha,
        repository,
    })
}

/// Converts a REST review and its reviewer identity to domain evidence.
pub fn normalize_review(thread: &ThreadId, review: RestReview) -> Result<Review, GitHubError> {
    let provider_id =
        ProviderId::new(review.id.to_string()).map_err(|_| GitHubError::InvalidProviderData)?;
    let submitted_at = review.submitted_at.map(parse_timestamp).transpose()?;
    let commit_sha = review
        .commit_id
        .filter(|value| !value.is_empty())
        .map(CommitSha::new)
        .transpose()
        .map_err(|_| GitHubError::InvalidProviderData)?;
    let reviewer = review.user.as_ref().and_then(normalize_reviewer);
    let mut provider_data = provider_data(review.extra);
    if let Some(user) = review.user {
        provider_data.insert("user", user);
    }
    Ok(Review {
        id: ReviewId::new(thread.clone(), provider_id),
        state: normalize_review_state(&review.state),
        reviewer,
        body: review.body,
        submitted_at,
        commit_sha,
        provider_data,
    })
}

/// Extracts an optional reviewer identity from provider user data.
pub fn normalize_reviewer(user: &Value) -> Option<ReviewerIdentity> {
    let object = user.as_object()?;
    let provider_id = object.get("id").and_then(provider_id_from_value);
    let login = object
        .get("login")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let provider_data = ProviderData::from_value(user.clone()).unwrap_or_default();
    Some(ReviewerIdentity {
        provider_id,
        login,
        provider_data,
    })
}

/// Reads a provider-issued opaque identity from a JSON value.
pub fn provider_id_from_value(value: &Value) -> Option<ProviderId> {
    let value = value
        .as_u64()
        .map(|id| id.to_string())
        .or_else(|| value.as_str().map(str::to_owned))?;
    ProviderId::new(value).ok()
}

/// Preserves unknown provider review states without inventing approval.
pub fn normalize_review_state(state: &str) -> ReviewState {
    match state {
        "APPROVED" => ReviewState::Approved,
        "CHANGES_REQUESTED" => ReviewState::ChangesRequested,
        "COMMENTED" => ReviewState::Commented,
        "DISMISSED" => ReviewState::Dismissed,
        "PENDING" => ReviewState::Pending,
        state => ReviewState::Other(state.to_owned()),
    }
}

/// Rejects provider timestamps outside the archive's UTC representation.
pub fn parse_timestamp(value: String) -> Result<UtcTimestamp, GitHubError> {
    UtcTimestamp::parse(&value).map_err(|_| GitHubError::InvalidProviderData)
}

/// Retains unmapped provider fields alongside normalized domain values.
pub fn provider_data(extra: BTreeMap<String, Value>) -> ProviderData {
    let mut provider_data = ProviderData::new();
    for (name, value) in extra {
        provider_data.insert(name, value);
    }
    provider_data
}

/// Hides raw provider payloads when converting a JSON decoding failure.
pub fn json_error(_: serde_json::Error) -> GitHubError {
    GitHubError::InvalidProviderData
}
