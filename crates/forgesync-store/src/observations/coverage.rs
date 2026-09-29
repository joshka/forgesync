//! Observation coverage operations.

use super::*;

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
