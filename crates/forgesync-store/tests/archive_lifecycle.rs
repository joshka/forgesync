//! # Archive lifecycle integration
//!
//! These cases cover explicit create, open, and migration behavior against an on-disk SQLite
//! archive. Opening must not silently create or migrate. This suite tests user-visible lifecycle
//! rules at the store boundary, including file and schema effects.
//!
//! Reopening, healthy diagnostics, and current-schema migration have independent scenarios.
//! Invalid history is arranged through raw SQLite connections, then checked through real archive
//! opening; inspection verifies that rejected opening leaves the damaged history unchanged.
//! Fixed SQL facts supply failed/deferred work for diagnostic projections without running sync.
//! The lease diagnostic uses current time because held-state reporting compares against the clock.
//! Unique filenames isolate concurrent cases; cleanup removes closed databases and sidecars.

//!
//! `access` owns reopen identity and missing-file behavior. `diagnostics` owns healthy probes and
//! explicit ledger/lease reporting. `migration` owns current and rejected schema history.
//! `fixture` supplies only filename, pool, and cleanup capabilities shared by those scenarios.

mod common;

#[path = "archive_lifecycle/access.rs"]
mod access;
#[path = "archive_lifecycle/diagnostics.rs"]
mod diagnostics;
#[path = "archive_lifecycle/fixture.rs"]
mod fixture;
#[path = "archive_lifecycle/migration.rs"]
mod migration;
