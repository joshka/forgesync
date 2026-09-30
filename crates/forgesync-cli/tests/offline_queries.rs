//! # Offline CLI query contract
//!
//! These scenarios exercise local query and presentation through the compiled process.
//! `keyword` and `advanced` distinguish literal queries from explicit FTS syntax.
//! `threads` checks archived selection and identity; `validation` checks argument failures.
//! `presentation` checks human section ordering without freezing dynamic diagnostic timestamps.
//!
//! JSON scenarios compare archive status before and after their own command. This establishes
//! reported-state stability while store/engine tests establish query and persistence mechanics.
//! `fixture` constructs independent archives and keeps setup separate from visible query execution.

#[path = "offline_queries/advanced.rs"]
mod advanced;
#[path = "offline_queries/fixture.rs"]
mod fixture;
#[path = "offline_queries/keyword.rs"]
mod keyword;
#[path = "offline_queries/presentation.rs"]
mod presentation;
#[path = "offline_queries/threads.rs"]
mod threads;
#[path = "offline_queries/validation.rs"]
mod validation;
