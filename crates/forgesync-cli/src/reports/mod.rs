//! # Present workflow results at the CLI boundary
//!
//! Each child module owns human wording and any command-specific JSON DTO for its workflow.
//! `archive` presents lifecycle and health results; `threads` and `detail` adapt discussion views;
//! `sync` presents acquisition/refresh outcomes and exit policy. `embedding`, `clusters`, and
//! `runs` keep their derived-data, local-decision, and durable-ledger output close to their
//! formatters.
//!
//! Commands import the owning report module directly. This root supplies no formatter prelude or
//! unrelated output types, so locating a DTO also locates its presentation contract. Engine/store
//! reports remain distinct from these process DTOs; JSON fields are deliberate command output,
//! while human summaries may condense the same evidence into a readable account.
//!
//! Report code performs no acquisition, archive write, or process configuration resolution. The
//! command owns cleanup and invokes presentation after its archive and advisory progress close.

pub mod archive;
pub mod clusters;
pub mod detail;
pub mod embedding;
mod pages;
pub mod run_detail;
pub mod runs;
pub mod sync;
pub mod threads;
mod timeline;
