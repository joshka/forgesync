//! Document source revision fixtures.

use serde_json::json;
use wiremock::MockServer;

use super::fixture_issues::{
    comment, issue_with_comment_count, mount_comments, mount_open_issues, mount_repository,
};

/// One provider revision of the fixed discussion and its comment.
pub struct DocumentSource<'a> {
    /// Parent source update used for document freshness, not acquisition time.
    pub issue_updated_at: &'a str,
    /// Comment source update; changing this alone does not change recipe text.
    pub comment_updated_at: &'a str,
    /// Reply text that contributes to the enriched document's content hash.
    pub comment_body: &'a str,
}

impl DocumentSource<'_> {
    /// Installs the repository, one issue, and one comment responses for this revision.
    ///
    /// This configures provider fixtures only. The caller must acquire the revision through sync
    /// before building or materializing documents from archive state.
    pub async fn mount(&self, server: &MockServer) {
        mount_repository(server).await;
        let mut issue =
            issue_with_comment_count(91, 11, "Document target", self.issue_updated_at, 1);
        issue["body"] = json!("Original discussion body");
        mount_open_issues(server, vec![issue]).await;
        let mut comment = comment(1101, self.comment_body);
        comment["updated_at"] = json!(self.comment_updated_at);
        mount_comments(server, 11, vec![comment]).await;
    }
}
