#![forbid(unsafe_code)]

//! Domain values shared by acquisition, storage, and local reads.
//!
//! A provider response enters through `forgesync-github`, becomes the checked values in this
//! crate, and is applied by `forgesync-store`. `forgesync-engine` builds workflows over those
//! values. Core never opens an archive, sends a request, or chooses a process configuration.
//!
//! The modules distinguish facts that are easy to conflate:
//!
//! - [`identity`] checks the names and numbers used to join records.
//! - [`content`] describes current normalized discussions and related resources.
//! - [`observation`] records when and how evidence was acquired; [`coverage`] says which resource
//!   family was complete, incomplete, unavailable, or deferred.
//! - [`outcome`] describes the result of a workflow without turning partial success into an error
//!   string. [`document`] and [`embedding`] hold derived retrieval inputs.
//!
//! Use checked constructors at external boundaries. A missing collection is not an observed empty
//! collection, and an acquisition sequence is not a provider timestamp. Those distinctions control
//! which archive rows may become canonical.
//!
//! ```
//! use forgesync_core::identity::GitHubHost;
//!
//! let host = GitHubHost::parse("https://github.com")?;
//! assert_eq!(host.as_str(), "github.com");
//! # Ok::<(), forgesync_core::identity::IdentityError>(())
//! ```
//!
//! # Build identities at the external boundary
//!
//! Use [`identity::RepositoryId`] for a provider repository identity and [`identity::ThreadId`] for
//! a discussion within it. A display URL or full repository name is not a replacement for that
//! durable identity. Checked constructors reject unusable provider IDs and zero thread numbers
//! before a workflow reaches SQL or an HTTP endpoint.
//!
//! # Keep source facts and acquisition evidence distinct
//!
//! [`content`] contains normalized provider facts. [`observation`] describes when those facts were
//! acquired and whether a resource collection was complete. [`coverage`] makes the result visible
//! to readers even when no canonical membership changed. The store's ordering policy consumes these
//! values; core does not decide which observation wins or start a retry.
//!
//! Derived [`document`] and [`embedding`] values carry their own identity and compatibility
//! context. Changing a retrieval recipe or service model does not change the source discussion, and
//! observing a fresh source revision does not automatically make an older vector compatible.
//!
//! The Rust API is still evolving with the application. Preserve the meaning of domain values when
//! refactoring; use the archive compatibility fixtures for persisted interpretation and the owning
//! module's examples for construction and validation.

pub mod content;
pub mod coverage;
pub mod document;
pub mod embedding;
pub mod identity;
pub mod observation;
pub mod outcome;
pub mod provider_data;
pub mod timestamp;
