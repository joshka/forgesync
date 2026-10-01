//! # Store read and search integration
//!
//! Local detail, filtered lists, full-text indexing, and migration-sensitive reads have separate
//! owners. `detail` assembles canonical content, timeline, and family coverage.
//! The `list_*` suites separate pagination, filters, and scope coverage.
//! Repository lookup, query validation, read-only search, and status counts have named owners.
//! `fts` checks that committed source updates replace searchable text.
//! `migration` checks explicit backfill and supported schema behavior.
//!
//! `fixture` constructs values and query settings; it performs no archive operation.
//! Every sequence reservation, observation write, and search call remains visible in its scenario.
//! Engine and CLI suites own request policy, derived ranking, and presentation.

mod common;

#[path = "inspect_search/detail.rs"]
mod detail;
#[path = "inspect_search/fixture.rs"]
mod fixture;
#[path = "inspect_search/fts.rs"]
mod fts;
#[path = "inspect_search/migration.rs"]
mod migration;

#[path = "inspect_search/list_coverage.rs"]
mod list_coverage;
#[path = "inspect_search/list_filters.rs"]
mod list_filters;
#[path = "inspect_search/list_pagination.rs"]
mod list_pagination;
#[path = "inspect_search/query_validation.rs"]
mod query_validation;
#[path = "inspect_search/read_only_search.rs"]
mod read_only_search;
#[path = "inspect_search/repository_lookup.rs"]
mod repository_lookup;
#[path = "inspect_search/status_counts.rs"]
mod status_counts;
