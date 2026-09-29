//! # Repository picker and its asynchronous read lifecycle
//!
//! [`RepositoryPicker`] keeps loaded repository rows, pending read identity, and picker selection
//! together. Cursor zero is the synthetic all-repositories row; later cursor positions map to
//! zero-based indices in `items`. Moving the cursor does not apply a filter until Enter is pressed.
//!
//! `applied` retains the repository selected for thread reads and writer actions. It is independent
//! of refreshed row order, so a newly inserted repository cannot retarget an acquisition command.
//! The app owns the resulting thread/detail reset, whereas this module owns repository loading,
//! stale-reply rejection, and cursor bounds.
//!
//! A failed refresh retains previously loaded rows and the applied filter. Its error is exposed to
//! the picker view and returned to the app's status line. Beginning a new read clears that error;
//! replies from an older generation change neither loaded rows nor current loading state.

use forgesync_core::content::Repository;

/// Loaded repository choices and selection state for the browser's left pane.
#[derive(Debug, Default)]
pub struct RepositoryPicker {
    /// Highlighted picker row, including the synthetic all-repositories row at zero.
    pub cursor: usize,
    /// Current archive repository rows, excluding the synthetic row.
    pub items: Vec<Repository>,
    /// Applied repository snapshot; `None` explicitly selects every repository.
    pub applied: Option<Repository>,
    /// Most recently started read identity, used to reject stale replies.
    pub generation: u64,
    /// Whether the current generation is awaiting a result; existing items may remain visible.
    pub loading: bool,
    /// Current read failure shown by the view; absent during a newly started read.
    pub error: Option<String>,
}

impl RepositoryPicker {
    /// Starts a new read while retaining loaded choices and the currently applied filter.
    pub fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.loading = true;
        self.error = None;
        self.generation
    }

    /// Applies the current generation and returns a failure for the app's status line.
    ///
    /// Stale replies are ignored, including their failures. Success clamps the highlighted row to
    /// the refreshed picker bounds; the applied repository remains independent of row order.
    pub fn apply(
        &mut self,
        generation: u64,
        result: Result<Vec<Repository>, String>,
    ) -> Option<String> {
        if generation != self.generation {
            return None;
        }
        self.loading = false;
        match result {
            Ok(items) => self.replace(items),
            Err(error) => self.fail(error),
        }
    }

    /// Replaces loaded choices and clamps the highlight, preserving the applied filter.
    fn replace(&mut self, items: Vec<Repository>) -> Option<String> {
        self.error = None;
        self.items = items;
        self.cursor = self.cursor.min(self.items.len());
        None
    }

    /// Retains existing rows while exposing the same safe error to the picker and status line.
    fn fail(&mut self, error: String) -> Option<String> {
        self.error = Some(error.clone());
        Some(error)
    }
}

#[cfg(test)]
mod tests;
