//! # Read directly recorded family coverage
//!
//! [`Archive::family_coverage`] looks up a canonical discussion by durable identity, then reads
//! the stored completeness state for one evidence family. An existing discussion without a family
//! row returns [`CoverageState::Missing`]; a missing discussion fails identity lookup instead.
//! Malformed stored state JSON is an error, not a missing-state fallback.
//!
//! This is a direct recorded-state read. It wraps the decoded state in [`Coverage`] without
//! comparing source timestamps or pull-request head context, so it does not derive staleness.
//! The richer read projections own those comparisons when presenting retained discussion evidence.
//! Callers should choose that projection when they need to judge whether recorded completeness is
//! still current rather than only inspecting the persisted family ledger.
//!
//! Identity lookup and the coverage query use separate pooled reads, not a transaction snapshot.
//! This operation writes no coverage, replaces no membership, and creates no observation. Family
//! acquisition and observation application own those mutations; the presence of child content
//! alone does not establish a complete collection.

use forgesync_core::coverage::{Coverage, CoverageState, EvidenceFamily};
use forgesync_core::identity::ThreadId;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::observations::{evidence_family_name, thread_row_id};

impl Archive {
    /// Reads the stored family state for an existing canonical discussion.
    ///
    /// An absent family row returns `Missing`; an absent discussion or invalid stored JSON fails.
    /// The returned coverage does not derive staleness from source timestamps or head context.
    /// Identity resolution and the state query are separate reads, not a frozen snapshot.
    pub async fn family_coverage(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
    ) -> Result<Coverage, StoreError> {
        let family_name = evidence_family_name(family);
        let mut connection = self.reader.acquire().await?;
        let thread_row_id = thread_row_id(&mut connection, thread).await?;
        let state_json: Option<String> = sqlx::query_scalar(
            "SELECT state_json FROM family_coverage WHERE thread_id = ? AND family = ?",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .fetch_optional(&self.reader)
        .await?;
        let state = match state_json {
            Some(json) => serde_json::from_str(&json)?,
            None => CoverageState::Missing,
        };
        Ok(Coverage::new(family, state))
    }
}
