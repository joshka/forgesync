//! # Focused sync scenarios
//!
//! These scenarios follow provider acquisition through engine coordination into durable archive
//! state. Each evidence family and follow-on stage has its own scenario module: enumeration,
//! comments, review failure/head freshness, review-thread membership/partial isolation, documents,
//! embeddings, refresh, and retry.
//!
//! Shared setup has explicit owners: `fixture_issues` constructs REST discussion responses and
//! local clients, `fixture_reviews` supplies pull-request head/review responses, and
//! `fixture_archive` provides local reads and database lifetime helpers. Scenarios import those
//! owners directly.
//!
//! The real workflow calls and their requests stay in each test; fixtures never run acquisition.
//! Read the affected scenario to see family selection, failure setup, and canonical-state
//! assertions. Document-only response setup stays alongside document materialization scenarios.

mod comments;
mod documents;
mod embeddings;
mod enumeration;
mod fixture_archive;
mod fixture_issues;
mod fixture_reviews;
mod refresh;
mod retry;
mod review_threads_membership;
mod review_threads_partial;
mod reviews_failure;
mod reviews_head_change;
