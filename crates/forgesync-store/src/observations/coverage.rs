//! # Persist evidence coverage separately from content
//!
//! Coverage writes describe which evidence family was observed, whether collection completed, and
//! why it may be incomplete. They accompany observations but must not be inferred from the mere
//! presence of a discussion or child row.
//!
//! Inspection and retry depend on this distinction. A source item may be current while a family is
//! missing or partial, and an incomplete acquisition must remain visible without replacing a known
//! complete membership.

use super::{
    Archive, Coverage, CoverageState, EvidenceFamily, StoreError, ThreadId, evidence_family_name,
    thread_row_id,
};

impl Archive {
    /// Reads the latest per-family completeness state; an absent row is `Missing`.
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
