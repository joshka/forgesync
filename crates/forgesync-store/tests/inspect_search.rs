//! # Store read and search integration
//!
//! Local detail, filtered lists, full-text indexing, and migration-sensitive reads have separate
//! owners. `detail` assembles canonical content, timeline, and family coverage.
//! `list_search` exercises scope, filters, ordering, pagination, and summary counts.
//! `fts` checks that committed source updates replace searchable text.
//! `migration` checks explicit backfill and supported schema behavior.
//!
//! `fixture` constructs values and query settings; it performs no archive operation.
//! Every sequence reservation, observation write, and search call remains visible in its scenario.
//! Engine and CLI suites own request policy, derived ranking, and presentation.

#[path = "inspect_search/detail.rs"]
mod detail;
#[path = "inspect_search/fixture.rs"]
mod fixture;
#[path = "inspect_search/fts.rs"]
mod fts;
#[path = "inspect_search/list_search.rs"]
mod list_search;
#[path = "inspect_search/migration.rs"]
mod migration;
