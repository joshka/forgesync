//! # Observation transaction integration
//!
//! Parent application, ordering, child completion, and rollback have separate scenario owners.
//! The store chooses canonical acquired evidence while retaining incomplete attempts independently.
//! `parents` covers normalized source/evidence high waters and payload acceptance.
//! `ordering` covers direct clock and acquisition-sequence comparisons without a database.
//! `children` covers paginated membership publication and complete-empty replacement.
//! `rollback` injects SQL failures and checks preserved canonical evidence and staged retry.
//!
//! `fixture` owns value construction, filename allocation, raw inspection, and cleanup.
//! Archive creation, repository registration, and sequence reservation remain visible in scenarios.
//! Scenario imports name that owner directly rather than depending on root imports.
//! Provider traversal and workflow scheduling remain engine integration responsibilities.

#[path = "observation_transactions/children.rs"]
mod children;
#[path = "observation_transactions/fixture.rs"]
mod fixture;
#[path = "observation_transactions/ordering.rs"]
mod ordering;
#[path = "observation_transactions/parents.rs"]
mod parents;
#[path = "observation_transactions/rollback.rs"]
mod rollback;
