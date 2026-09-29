//! # Archive coverage projection and refresh state
//!
//! [`CoveragePanel`] owns the latest archive-status projection and the independent generation of
//! its read. The coverage screen uses this archive-wide view rather than deriving completeness
//! from whatever discussion page happens to be visible in the browser.
//!
//! Beginning a refresh retains the last projection while clearing the prior failure. A current
//! success replaces it; failure remains visible without discarding the last acquired projection.
//! Rendering decides whether to show loading or the failure before using retained data.
//!
//! Query tasks perform read-only archive inspection. This owner rejects stale replies and returns
//! current failures to the app's status line; it does not trigger sync, change coverage, or infer
//! that stored discussion content proves complete evidence.

use forgesync_store::reads::ArchiveStatus;

/// Archive-wide coverage data with its own pending read and safe failure state.
#[derive(Debug, Default)]
pub struct CoveragePanel {
    /// Last successful projection, retained while refreshing or after a read failure.
    pub data: Option<ArchiveStatus>,
    /// Most recently started coverage read, independent of discussion paging.
    pub generation: u64,
    /// Whether the current read is pending; retained data may still exist.
    pub loading: bool,
    /// Current safe read failure, cleared before a new request or successful replacement.
    pub error: Option<String>,
}

impl CoveragePanel {
    /// Starts an archive-wide read without discarding the previous successful projection.
    pub fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.loading = true;
        self.error = None;
        self.generation
    }

    /// Replaces only current-generation coverage and returns its failure for the app's status.
    pub fn apply(
        &mut self,
        generation: u64,
        result: Result<Box<ArchiveStatus>, String>,
    ) -> Option<String> {
        if generation != self.generation {
            return None;
        }
        self.loading = false;
        match result {
            Ok(status) => self.replace(*status),
            Err(error) => self.fail(error),
        }
    }

    /// Stores a successful archive projection and removes a prior refresh error.
    fn replace(&mut self, status: ArchiveStatus) -> Option<String> {
        self.data = Some(status);
        self.error = None;
        None
    }

    /// Records a refresh failure while retaining the last successful archive projection.
    fn fail(&mut self, error: String) -> Option<String> {
        self.error = Some(error.clone());
        Some(error)
    }
}

#[cfg(test)]
mod tests;
