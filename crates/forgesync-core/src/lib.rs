#![forbid(unsafe_code)]

//! Domain vocabulary shared by Forgesync's acquisition, storage, and read workflows.
//!
//! [`identity`] defines checked GitHub and archive identifiers. [`content`] holds normalized
//! discussions and their related resources. [`observation`] and [`coverage`] keep acquisition
//! order and completeness explicit so an incomplete response cannot silently replace a complete
//! collection. [`document`] and [`embedding`] describe local search inputs.
//!
//! ```
//! use forgesync_core::identity::GitHubHost;
//!
//! let host = GitHubHost::parse("https://github.com")?;
//! assert_eq!(host.as_str(), "github.com");
//! # Ok::<(), forgesync_core::identity::IdentityError>(())
//! ```

pub mod content;
pub mod coverage;
pub mod document;
pub mod embedding;
pub mod identity;
pub mod observation;
pub mod outcome;
pub mod provider_data;
pub mod timestamp;
