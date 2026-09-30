//! # Cluster workflow integration
//!
//! These scenarios follow current archive evidence through engine analysis to persisted groups.
//! Complete creation, partial preservation, and unavailable vector namespace rejection have shallow
//! sibling owners so each file presents one contract and its assertions.
//!
//! Each scenario shows reservation, observation application, fenced representation writes, lease
//! release, and engine operations. `fixture` constructs payloads and owns database-path lifetime;
//! it never hides archive mutations or computes expected clustering results.
//!
//! Pure scoring and candidate policy remain beside the production implementation. Store suites
//! cover durable generation application, retirement, and human decisions. These integration cases
//! connect representation selection and engine coverage policy to public store projections.

#[path = "clustering_workflow/complete.rs"]
mod complete;
#[path = "clustering_workflow/fixture.rs"]
mod fixture;
#[path = "clustering_workflow/namespace.rs"]
mod namespace;
#[path = "clustering_workflow/partial.rs"]
mod partial;
