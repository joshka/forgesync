//! # Observation transaction integration
//!
//! Parent application, ordering, child completion, and rollback have separate scenario owners.
//! The store chooses canonical acquired evidence while retaining incomplete attempts independently.
//! `parent_high_water` covers independent source/evidence selection; `parent_integrity` covers
//! replay and rejected conflicts.
//! `ordering` covers direct clock and acquisition-sequence comparisons without a database.
//! The `child_*` suites isolate replay, partial/empty attempts, head snapshots, and supersession.
//! `rollback_comments` and `rollback_review_threads` isolate their atomic publication contracts.
//!
//! `fixture` owns value construction, filename allocation, raw inspection, and cleanup.
//! Archive creation, repository registration, and sequence reservation remain visible in scenarios.
//! Scenario imports name that owner directly rather than depending on root imports.
//! Provider traversal and workflow scheduling remain engine integration responsibilities.

mod common;

#[path = "observation_transactions/fixture.rs"]
mod fixture;
#[path = "observation_transactions/ordering.rs"]
mod ordering;

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

#[path = "observation_transactions/parent_high_water.rs"]
mod parent_high_water;
#[path = "observation_transactions/parent_integrity.rs"]
mod parent_integrity;
#[path = "observation_transactions/rollback_comments.rs"]
mod rollback_comments;
#[path = "observation_transactions/rollback_review_threads.rs"]
mod rollback_review_threads;
