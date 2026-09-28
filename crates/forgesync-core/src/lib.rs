#![forbid(unsafe_code)]

//! Domain vocabulary and contracts shared by Forgesync application crates.

mod content;
mod coverage;
mod document;
mod identity;
mod observation;
mod outcome;
mod provider_data;
mod timestamp;

pub use content::{
    BranchRef, Comment, Discussion, PullRequestMetadata, Repository, Review, ReviewState,
    ReviewThread, ReviewerIdentity, SourceState, ThreadKind,
};
pub use coverage::{
    Coverage, CoverageState, DeferredReason, EvidenceFamily, Failure, FailureKind,
    UnavailableReason,
};
pub use document::{Document, DocumentRecipe};
pub use identity::{
    CommentId, CommitSha, GitHubHost, IdentityError, ObservationSequence, ProviderId, RepositoryId,
    ReviewId, ReviewThreadId, RunId, ThreadId, ThreadNumber, ThreadReference,
};
pub use observation::{CollectionCompleteness, IncompleteReason, Observation, SourceClock};
pub use outcome::OperationOutcome;
pub use provider_data::ProviderData;
pub use timestamp::{TimestampError, UtcTimestamp};
