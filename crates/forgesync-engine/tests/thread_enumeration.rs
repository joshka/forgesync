//! # Thread enumeration integration
//!
//! These provider-to-archive scenarios protect repository enumeration and parent evidence
//! retention. `partial` follows repository redirection and a later-page failure; `replay` proves
//! that two complete scans retain canonical identity and content without duplicate rows.
//!
//! Each scenario shows provider pages, the actual engine operation, local reads, and durable scan
//! assertions. `fixture` owns client/payload construction, a stable repository mock, and database
//! lifetime. It never invokes enumeration or hides archive mutation.
//!
//! Enumeration establishes parent coverage, not child-family completeness. Focused store completion
//! tests enforce publication invariants, while sync workflow suites add acquisition selection,
//! checkpoints, and child-family policy above this lower-level engine boundary.

#[path = "thread_enumeration/fixture.rs"]
mod fixture;
#[path = "thread_enumeration/partial.rs"]
mod partial;
#[path = "thread_enumeration/replay.rs"]
mod replay;
