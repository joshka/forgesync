//! # Selected discussion detail and its read lifecycle
//!
//! [`DetailPane`] owns the selected discussion projection, read generation, and requested scroll
//! position. [`DetailState`] distinguishes no selection, pending acquisition, loaded detail, and
//! failure; content, loading, and an error cannot contradict each other.
//!
//! Changing thread selection or starting a new list invalidates this pane before another detail
//! read is requested. Beginning detail advances the generation and shows loading with no old
//! content. Replies from older generations are ignored even if they completed successfully.
//!
//! Query tasks own archive access. The app applies a current result and forwards any safe failure
//! to its status line. Input changes scroll intent; rendering clamps that intent to visible content
//! after resize. Success resets scrolling to the top of the newly selected discussion.
//! Loaded details remain boxed as they arrive through the message channel, keeping the state enum
//! small without copying the discussion and its evidence collections.

use forgesync_store::reads::ThreadDetail;

/// Selected detail lifecycle and requested scrolling, independent of browser list coordinates.
#[derive(Debug, Default)]
pub struct DetailPane {
    /// Latest selection/read generation; invalidation advances it even without starting a task.
    pub generation: u64,
    /// Requested vertical offset, clamped by rendering to the current content and viewport.
    pub scroll: u16,
    /// Mutually exclusive empty, loading, ready, or failed presentation.
    pub state: DetailState,
}

/// Current-generation detail presentation, with data only in the ready state.
#[derive(Debug, Default)]
pub enum DetailState {
    /// No selected detail and no pending read or failure.
    #[default]
    Empty,
    /// A detail read is pending; obsolete content has already been removed.
    Loading,
    /// Current selected discussion and its canonical evidence.
    Ready(Box<ThreadDetail>),
    /// Safe read failure; no discussion content remains available for display.
    Failed(String),
}

impl DetailPane {
    /// Invalidates old replies and clears content, errors, loading, and scrolling on selection
    /// change.
    pub fn invalidate(&mut self) {
        self.generation += 1;
        self.scroll = 0;
        self.state = DetailState::Empty;
    }

    /// Starts detail acquisition after invalidating any previous selection or read.
    pub fn begin(&mut self) -> u64 {
        self.invalidate();
        self.state = DetailState::Loading;
        self.generation
    }

    /// Applies only the latest generation, returning a failure for the app's status line.
    pub fn apply(
        &mut self,
        generation: u64,
        result: Result<Box<ThreadDetail>, String>,
    ) -> Option<String> {
        if generation != self.generation {
            return None;
        }
        match result {
            Ok(detail) => self.show(detail),
            Err(error) => self.fail(error),
        }
    }

    /// Stores the selected projection and resets scrolling for its new content.
    fn show(&mut self, detail: Box<ThreadDetail>) -> Option<String> {
        self.state = DetailState::Ready(detail);
        self.scroll = 0;
        None
    }

    /// Removes content and retains the same safe failure shown by the app's status line.
    fn fail(&mut self, error: String) -> Option<String> {
        self.state = DetailState::Failed(error.clone());
        Some(error)
    }

    /// Returns current loaded detail, absent during loading, failure, or no selection.
    pub fn content(&self) -> Option<&ThreadDetail> {
        match &self.state {
            DetailState::Ready(detail) => Some(detail.as_ref()),
            _ => None,
        }
    }

    /// Reports whether the current selected discussion is awaiting its detail result.
    pub fn is_loading(&self) -> bool {
        matches!(self.state, DetailState::Loading)
    }

    /// Returns a safe detail-read failure, absent for empty, pending, or ready states.
    pub fn error(&self) -> Option<&str> {
        match &self.state {
            DetailState::Failed(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
