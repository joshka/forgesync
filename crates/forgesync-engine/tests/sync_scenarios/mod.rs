//! # Focused sync scenarios
//!
//! These scenarios follow provider acquisition through engine coordination into durable archive
//! state. Each evidence family and follow-on stage has its own scenario module: enumeration,
//! comments, review failure/head freshness, review-thread membership/partial isolation, documents,
//! embeddings, refresh, and retry.
//!
//! Shared setup has explicit owners: `fixture_issues` constructs REST discussion responses and
//! local clients, `fixture_reviews` supplies pull-request head/review responses, and
//! `fixture_archive` constructs references, selects already-read coverage, and owns database
//! lifetime. `fixture_documents` names document source revisions. Scenarios import these owners
//! directly. `fixture_embeddings` owns service response policy and pure client construction.
//!
//! The real workflow calls and their requests stay in each test; fixtures never run acquisition.
//! Read the affected scenario to see family selection, failure setup, and canonical-state
//! assertions. Document source setup configures HTTP responses; materialization stays in its
//! scenario.

mod closed_sweep;
mod comments_empty;
mod comments_ledger;
mod comments_retry;
mod documents;
mod embedding_retry;
mod enumeration_replay;
mod fixture_archive;
mod fixture_documents;
mod fixture_embeddings;
mod fixture_issues;
mod fixture_reviews;
mod hybrid_search;
mod keyword_fallback;
mod refresh;
mod retry;
mod review_threads_membership;
mod review_threads_partial;
mod reviews_failure;
mod reviews_head_change;
mod run_creation;
