//! Typed failures at the provider and transport boundary.
//!
//! [`GitHubError`] distinguishes configuration, origin, transport, HTTP, pagination, GraphQL, and
//! provider-data failures. [`ApiFailureKind`] classifies an API response for retry and reporting
//! without requiring callers to inspect response text.
//!
//! The transport creates errors from requests and responses; resource modules add context when a
//! provider payload lacks required fields or violates a relationship. The engine turns these
//! errors into structured family failures and partial run reports. A failure after earlier pages
//! have been committed does not imply that the whole run had no useful result.
//!
//! Error values deliberately avoid storing credentials, headers, discussion bodies, or raw
//! payloads. Match variants or categories when making a retry decision; displayed messages are
//! diagnostics, not a stable machine protocol.

use std::time::Duration;

use thiserror::Error;

/// Stable classification for an unsuccessful GitHub API response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiFailureKind {
    /// The request did not include an accepted credential.
    AuthenticationRequired,
    /// The authenticated account cannot access this resource.
    PermissionDenied,
    /// The requested resource does not exist or is not visible.
    NotFound,
    /// The request conflicts with current provider state.
    Conflict,
    /// GitHub reported an API rate limit.
    RateLimited,
    /// GitHub rejected the request for another client-side reason.
    Client,
    /// GitHub returned a server-side failure.
    Server,
}

/// Provider transport and API failures without response payloads or credentials.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum GitHubError {
    /// The operation was cancelled by its caller.
    #[error("GitHub request was cancelled")]
    Cancelled,
    /// The configured request timeout elapsed.
    #[error("GitHub request timed out")]
    Timeout,
    /// A connection or transport failure prevented a response.
    #[error("GitHub request failed due to a network error")]
    Network,
    /// The API returned an unsuccessful HTTP response.
    #[error("GitHub API returned HTTP {status} ({kind:?})")]
    Api {
        /// HTTP status code.
        status: u16,
        /// Typed provider failure.
        kind: ApiFailureKind,
    },
    /// A provider wait exceeds the remaining automatic retry budget.
    #[error("GitHub retry budget was exhausted; provider wait: {retry_after:?}")]
    Deferred {
        /// Delay requested by GitHub, when supplied.
        retry_after: Option<Duration>,
    },
    /// A request or pagination link points outside the configured API origin.
    #[error("GitHub URL is outside the configured API origin")]
    UntrustedOrigin,
    /// A GitHub pagination Link header cannot be parsed safely.
    #[error("GitHub pagination link is invalid")]
    InvalidPaginationLink,
    /// The provider returned a redirect which must be followed explicitly after validation.
    #[error("GitHub redirected the request; validate the destination and retry explicitly")]
    RedirectRejected,
    /// A successful API response exceeded the local response size limit.
    #[error("GitHub response exceeded the configured body limit")]
    ResponseTooLarge,
    /// The API returned a successful response that was not valid JSON for the requested type.
    #[error("GitHub response was not valid JSON")]
    InvalidJson,
    /// GitHub returned one or more GraphQL errors, possibly with partial data.
    #[error("GitHub GraphQL response contained {count} error(s)")]
    GraphqlErrors {
        /// Number of errors in the GraphQL response.
        count: u32,
    },
    /// Provider data could not be normalized into the selected core model.
    #[error("GitHub response contains invalid provider data")]
    InvalidProviderData,
    /// The configured request concurrency limit could not be acquired.
    #[error("GitHub request capacity is unavailable")]
    ConcurrencyUnavailable,
    /// The API base URL was invalid or unsafe.
    #[error("GitHub API base URL must be HTTPS (loopback HTTP is allowed for local fixtures)")]
    InvalidApiBaseUrl,
    /// The request timeout, concurrency limit, or retry budget is invalid.
    #[error("GitHub client configuration is invalid")]
    InvalidConfiguration,
    /// Reqwest could not construct the shared HTTP client.
    #[error("GitHub HTTP client initialization failed")]
    ClientInitialization,
}
