//! # Sync workflow integration
//!
//! This suite exercises acquisition from a provider fixture through engine coordination into the
//! archive. It keeps the end-to-end contract visible: normalized evidence, partial outcomes, and
//! durable run state. Focused scenarios under `sync_scenarios` isolate family and stage behavior.

mod sync_scenarios;
