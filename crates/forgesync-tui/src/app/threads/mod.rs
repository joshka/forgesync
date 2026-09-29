//! # Discussion page, selection, and asynchronous read state
//!
//! [`ThreadList`] owns the current discussion rows, selected row, pagination position, and read
//! lifecycle. [`ThreadReply`] keeps a read's generation, requested offset, and result together so
//! applying a page cannot accidentally use the coordinates of a different request.
//!
//! Beginning a read clears rows and selection: a new search or repository scope must not show old
//! discussions as its results. The app separately invalidates detail for that scope change. A
//! successful current reply selects the first row when available and preserves the store's next
//! offset; an empty page has no selection. Failure clears rows and pagination and exposes an error.
//!
//! Stale replies do not change loading, offset, selection, or errors. Navigation and rendering use
//! these facts locally, while query tasks own archive access and the app coordinates focus/detail.
//! Page coverage remains part of the store result; the dedicated coverage screen reads its own
//! archive projection instead of treating a list page as complete archive diagnostics.

use forgesync_store::reads::{ThreadPage, ThreadSummary};

/// Current discussion page and selection, scoped to the latest requested read generation.
#[derive(Debug, Default)]
pub struct ThreadList {
    /// Rows in the engine's stable result order.
    pub items: Vec<ThreadSummary>,
    /// Selected item index; absent while loading, after failure, or on an empty page.
    pub selected: Option<usize>,
    /// Offset of the latest applied page, also used when reloading that page.
    pub offset: u64,
    /// Store-provided continuation offset, absent when no later result page is available.
    pub next_offset: Option<u64>,
    /// Most recently started read, independent of repository and detail generations.
    pub generation: u64,
    /// Whether the current read is pending; input remains responsive while it runs.
    pub loading: bool,
    /// Current safe read failure, cleared before a new read or successful replacement.
    pub error: Option<String>,
}

/// One page response with the request coordinates needed for stale-result rejection.
pub struct ThreadReply {
    /// Thread-list generation reserved before the read task started.
    pub generation: u64,
    /// Requested page offset, applied only if the generation is still current.
    pub offset: u64,
    /// Loaded page or safe presentation error; boxed to limit the message enum's inline size.
    pub result: Result<Box<ThreadPage>, String>,
}

impl ThreadList {
    /// Starts a read and clears obsolete rows, selection, continuation, and prior failure.
    ///
    /// The applied offset remains until the response arrives. The app invalidates the old detail
    /// as part of the same request transition, because this owner does not control another panel.
    pub fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.loading = true;
        self.error = None;
        self.items.clear();
        self.selected = None;
        self.next_offset = None;
        self.generation
    }

    /// Applies a current page and returns its failure for the app's status line.
    pub fn apply(&mut self, reply: ThreadReply) -> Option<String> {
        if reply.generation != self.generation {
            return None;
        }
        self.loading = false;
        self.offset = reply.offset;
        match reply.result {
            Ok(page) => self.replace(*page),
            Err(error) => self.fail(error),
        }
    }

    /// Selects the first available result and takes continuation from the actual store page.
    fn replace(&mut self, page: ThreadPage) -> Option<String> {
        self.error = None;
        self.items = page.items;
        self.next_offset = page.next_offset;
        self.selected = (!self.items.is_empty()).then_some(0);
        None
    }

    /// Clears unusable rows and continuation while retaining the requested offset for retry.
    fn fail(&mut self, error: String) -> Option<String> {
        self.error = Some(error.clone());
        self.items.clear();
        self.next_offset = None;
        self.selected = None;
        Some(error)
    }
}

#[cfg(test)]
mod tests;
