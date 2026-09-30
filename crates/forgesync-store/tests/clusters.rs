//! # Cluster persistence integration
//!
//! This suite separates derived generations from local maintainer decisions and invalid inputs.
//! `generations` checks identity reuse and retention of explicit local decisions.
//! `partial_generation` preserves an omitted group; `complete_generation` retires that group.
//! `canonical` checks that a stored discussion outside the group cannot become canonical.
//! `coverage` rejects falsely complete vector coverage and verifies no cluster was persisted.
//!
//! Every scenario shows public archive writes and observations directly.
//! `fixture` supplies normalized values, read requests, paths, and closed-resource cleanup only.
//! Engine tests own scoring and proposal construction; these cases own durable representation.
//! Restoration has its own public-operation regression suite in `cluster_restoration.rs`.

#[path = "clusters/canonical.rs"]
mod canonical;
#[path = "clusters/coverage.rs"]
mod coverage;
#[path = "clusters/fixture.rs"]
mod fixture;
#[path = "clusters/generations.rs"]
mod generations;

#[path = "clusters/complete_generation.rs"]
mod complete_generation;
#[path = "clusters/partial_generation.rs"]
mod partial_generation;
