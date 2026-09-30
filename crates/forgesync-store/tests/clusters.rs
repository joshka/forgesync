//! # Cluster persistence integration
//!
//! This suite separates derived generations from local maintainer decisions and invalid inputs.
//! `generations` checks replacement, identity reuse, decision retention, and retirement.
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
