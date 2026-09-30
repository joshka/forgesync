//! # Observation transaction integration
//!
//! Parent application, ordering, child completion, and rollback have separate scenario owners.
//! The store chooses canonical acquired evidence while retaining incomplete attempts independently.
//! `parents` covers normalized source/evidence high waters and payload acceptance.
//! `ordering` covers direct clock and acquisition-sequence comparisons without a database.
//! The `child_*` suites isolate replay, partial/empty attempts, head snapshots, and supersession.
//! `rollback` injects SQL failures and checks preserved canonical evidence and staged retry.
//!
//! `fixture` owns value construction, filename allocation, raw inspection, and cleanup.
//! Archive creation, repository registration, and sequence reservation remain visible in scenarios.
//! Scenario imports name that owner directly rather than depending on root imports.
//! Provider traversal and workflow scheduling remain engine integration responsibilities.

#[path = "observation_transactions/fixture.rs"]
mod fixture;
#[path = "observation_transactions/ordering.rs"]
mod ordering;
#[path = "observation_transactions/parents.rs"]
mod parents;
#[path = "observation_transactions/rollback.rs"]
mod rollback;

#[path = "observation_transactions/child_empty_attempt.rs"]
mod child_empty_attempt;
#[path = "observation_transactions/child_partial.rs"]
mod child_partial;
#[path = "observation_transactions/child_replay.rs"]
mod child_replay;
#[path = "observation_transactions/child_snapshots.rs"]
mod child_snapshots;
#[path = "observation_transactions/child_supersession.rs"]
mod child_supersession;
