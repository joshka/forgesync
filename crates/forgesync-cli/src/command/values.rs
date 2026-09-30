//! # CLI value vocabulary at the parsing boundary
//!
//! These enums define named choices accepted by Clap: evidence families, discussion kinds and
//! states, ordering, search modes, acquisition scope, refresh analysis, color, and log encoding.
//! Their variant comments describe the user's choice; command adapters convert those choices into
//! core or engine request types without making those libraries depend on Clap.
//!
//! [`RunFamilyArg`] has a direct evidence-family conversion because each variant names a ledger
//! scope. Other choices are interpreted by their command or presentation owner, where defaults and
//! operation-specific restrictions are visible. A parsed value does not prove that the selected
//! operation is valid for the archive, configured service, or available evidence.
//!
//! Query filters and acquisition scope are distinct vocabularies: a local state filter selects
//! retained rows, while a sync state selects provider acquisition. Similarly, search mode chooses
//! an execution policy as well as query interpretation. Semantic and hybrid modes can request a
//! query embedding from the configured service even though discussion evidence is read locally.
//!
//! Pure variant-to-variant conversions stay exhaustive and inline so readers can inspect the whole
//! vocabulary mapping. Behavioral work belongs with the selected command, not inside these enums.

use clap::ValueEnum;
use forgesync_core::coverage::EvidenceFamily;

/// Evidence family accepted by explicit run retry filters.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum RunFamilyArg {
    /// Repository discussion enumeration.
    Threads,
    /// Discussion comments.
    Comments,
    /// Pull-request base and head metadata.
    PullRequestMetadata,
    /// Submitted pull-request reviews.
    Reviews,
    /// Current pull-request review threads.
    ReviewThreads,
}

impl From<RunFamilyArg> for EvidenceFamily {
    /// Maps parsed retry-family selection to domain evidence, retaining metadata and review
    /// families as distinct ledger scopes.
    fn from(value: RunFamilyArg) -> Self {
        match value {
            RunFamilyArg::Threads => Self::Threads,
            RunFamilyArg::Comments => Self::Comments,
            RunFamilyArg::PullRequestMetadata => Self::PullRequestMetadata,
            RunFamilyArg::Reviews => Self::Reviews,
            RunFamilyArg::ReviewThreads => Self::ReviewThreads,
        }
    }
}

/// Discussion kind accepted by local query filters.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ThreadKindArg {
    /// GitHub issue.
    Issue,
    /// GitHub pull request.
    Pr,
}

/// Source state accepted by local query filters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum ThreadStateArg {
    /// Include open, closed, and unrecognized source states.
    #[default]
    All,
    /// Include source-open discussions.
    Open,
    /// Include source-closed discussions.
    Closed,
}

/// Sort order accepted by local query commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ThreadSortArg {
    /// Rank FTS matches first.
    Relevance,
    /// Sort by source update time, newest first.
    Updated,
    /// Sort by source creation time, newest first.
    Created,
}

/// Search execution and expression interpretation selected for one query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum SearchModeArg {
    /// Quote ordinary text tokens and treat punctuation as separators.
    #[default]
    Keyword,
    /// Rank current compatible document vectors by exact cosine similarity.
    Semantic,
    /// Fuse keyword and semantic result ranks.
    Hybrid,
    /// Accept FTS5 phrases, boolean operators, and grouping syntax.
    AdvancedFts,
}

/// Thread-state selection accepted by sync.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum SyncThreadStateArg {
    /// Fetch only open threads.
    Open,
    /// Fetch only closed threads, using the successful closed-sweep watermark.
    Closed,
    /// Fetch all open and closed threads.
    All,
}

/// Optional evidence family selected for a sync run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum SyncIncludeArg {
    /// Acquire issue and pull-request discussion comments.
    Comments,
    /// Acquire pull-request reviews and reviewer identities.
    Reviews,
    /// Acquire current pull-request review threads and nested comments through GraphQL.
    ReviewThreads,
}

/// Optional analysis stage accepted by refresh.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum RefreshAnalysisArg {
    /// Build current documents and request missing embedding vectors.
    #[value(name = "embeddings")]
    Embeddings,
    /// Generate deterministic clusters from compatible stored vectors.
    Clusters,
}

/// Terminal color selection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum ColorChoice {
    /// Detect whether stdout is a terminal and respect NO_COLOR.
    #[default]
    Auto,
    /// Always use terminal colors for human output.
    Always,
    /// Never use terminal colors.
    Never,
}

/// Diagnostic log encoding.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum LogFormat {
    /// Human-readable diagnostics.
    #[default]
    Text,
    /// Structured JSON diagnostics.
    Json,
}
