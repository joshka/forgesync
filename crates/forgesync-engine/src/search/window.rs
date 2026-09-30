//! # Bounded windows for ranked retrieval
//!
//! [`SearchWindow`] validates the requested offset and page size before candidate acquisition.
//! Ranked search must collect every candidate preceding the requested page, plus one result to
//! determine whether another page exists. A page size alone therefore cannot bound its work.
//!
//! The window retains both presentation coordinates and the acquisition limit, keeping their
//! relationship explicit. Keyword-only search uses store pagination directly; semantic and hybrid
//! retrieval use this window before embedding the query or reading candidate vectors.
//!
//! The maximum bounds total ranking work, not merely the number of results returned to a caller.
//! Conversion and addition failures have the same public classification as an oversized window.

use crate::error::EngineError;
use crate::query::checked_page;

/// Largest offset-plus-page-size accepted by exact ranked retrieval.
const MAX_SEARCH_WINDOW: usize = 10_000;

/// Validated page coordinates and the candidate prefix needed to produce that page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchWindow {
    /// First result to include after ranking the complete candidate prefix.
    pub offset: u64,
    /// Maximum number of results presented on this page.
    pub limit: u32,
    /// Prefix length including one extra result for continuation detection.
    pub candidate_limit: usize,
}

impl SearchWindow {
    /// Validates ordinary pagination and bounds the complete ranking prefix.
    ///
    /// Invalid page sizes and offsets retain the shared pagination errors. Valid offsets that
    /// cannot fit in memory,
    /// overflow during addition, or exceed the ranking budget return `SearchWindowTooLarge`.
    pub fn new(limit: u32, offset: u64) -> Result<Self, EngineError> {
        let (limit, offset) = checked_page(limit, offset)?;
        let prefix = usize::try_from(offset)
            .ok()
            .and_then(|offset| offset.checked_add(usize::try_from(limit.get()).ok()?))
            .filter(|prefix| *prefix <= MAX_SEARCH_WINDOW)
            .ok_or(EngineError::SearchWindowTooLarge)?;
        Ok(Self {
            offset,
            limit: limit.get(),
            candidate_limit: prefix + 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::error::EngineError;
    use crate::search::window::SearchWindow;

    #[test]
    fn candidate_prefix_includes_skipped_results_and_continuation_probe() {
        let window = SearchWindow::new(20, 40).expect("valid page");
        assert_eq!(window.offset, 40);
        assert_eq!(window.limit, 20);
        assert_eq!(window.candidate_limit, 61);
    }

    #[test]
    fn maximum_ranking_window_still_has_a_continuation_probe() {
        let window = SearchWindow::new(20, 9_980).expect("maximum window");
        assert_eq!(window.candidate_limit, 10_001);
    }

    #[test]
    fn page_beyond_ranking_budget_is_rejected() {
        assert!(matches!(
            SearchWindow::new(20, 9_981),
            Err(EngineError::SearchWindowTooLarge)
        ));
    }

    #[test]
    fn invalid_store_offset_retains_its_pagination_error() {
        assert!(matches!(
            SearchWindow::new(20, u64::MAX),
            Err(EngineError::InvalidPageOffset)
        ));
    }
}
