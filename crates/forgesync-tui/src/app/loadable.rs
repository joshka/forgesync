//! Generation-tagged read state shared by every panel.

/// Data from the latest successful read, plus the state of the read that may replace it.
///
/// Every request takes a new generation; replies carrying an older generation are ignored, so a
/// late result cannot overwrite a newer scope or selection.
#[derive(Debug, Default)]
pub struct Loadable<T> {
    pub data: T,
    pub generation: u64,
    pub loading: bool,
    /// Failure of the current read, shown in place of (but without discarding) `data`.
    pub error: Option<String>,
}

impl<T> Loadable<T> {
    /// Starts a read, keeping existing data visible until the reply arrives.
    pub fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.loading = true;
        self.error = None;
        self.generation
    }

    /// Applies a reply to the current generation, copying a failure to `status`.
    ///
    /// Returns `false` for a stale reply, which changes nothing.
    pub fn apply(
        &mut self,
        generation: u64,
        result: Result<T, String>,
        status: &mut Option<String>,
    ) -> bool {
        if generation != self.generation {
            return false;
        }
        self.loading = false;
        match result {
            Ok(data) => {
                self.data = data;
                self.error = None;
            }
            Err(error) => {
                *status = Some(error.clone());
                self.error = Some(error);
            }
        }
        true
    }
}

impl<T> Loadable<T> {
    /// Test shorthand for a panel that has already loaded `data`.
    #[cfg(test)]
    pub fn loaded(data: T) -> Self {
        Self {
            data,
            generation: 0,
            loading: false,
            error: None,
        }
    }
}

impl<T: Default> Loadable<T> {
    /// Discards data and rejects any pending reply without starting another read.
    pub fn reset(&mut self) {
        self.generation += 1;
        self.loading = false;
        self.error = None;
        self.data = T::default();
    }
}

#[cfg(test)]
mod tests {
    use super::Loadable;

    fn loaded(data: u32) -> Loadable<u32> {
        Loadable {
            data,
            error: Some("previous failure".to_owned()),
            ..Loadable::default()
        }
    }

    #[test]
    fn begin_keeps_data_and_clears_the_previous_error() {
        let mut panel = loaded(7);

        assert_eq!(panel.begin(), 1);

        assert_eq!(panel.data, 7);
        assert!(panel.loading);
        assert_eq!(panel.error, None);
    }

    #[test]
    fn current_success_replaces_data_and_finishes_loading() {
        let mut panel = loaded(7);
        let mut status = None;
        let generation = panel.begin();

        assert!(panel.apply(generation, Ok(8), &mut status));

        assert_eq!(panel.data, 8);
        assert!(!panel.loading);
        assert_eq!(panel.error, None);
        assert_eq!(status, None);
    }

    #[test]
    fn current_failure_keeps_data_and_reports_the_error() {
        let mut panel = loaded(7);
        let mut status = None;
        let generation = panel.begin();

        assert!(panel.apply(generation, Err("read failed".to_owned()), &mut status));

        assert_eq!(panel.data, 7);
        assert!(!panel.loading);
        assert_eq!(panel.error.as_deref(), Some("read failed"));
        assert_eq!(status.as_deref(), Some("read failed"));
    }

    #[rstest::rstest]
    #[case::success(Ok(8))]
    #[case::failure(Err("stale failure".to_owned()))]
    fn stale_reply_changes_nothing(#[case] result: Result<u32, String>) {
        let mut panel = loaded(7);
        let mut status = None;
        let stale = panel.begin();
        panel.begin();

        assert!(!panel.apply(stale, result, &mut status));

        assert_eq!(panel.data, 7);
        assert!(panel.loading);
        assert_eq!(panel.error, None);
        assert_eq!(status, None);
    }

    #[test]
    fn reset_rejects_the_pending_reply() {
        let mut panel = loaded(7);
        let mut status = None;
        let pending = panel.begin();

        panel.reset();

        assert!(!panel.apply(pending, Ok(8), &mut status));
        assert_eq!(panel.data, 0);
        assert!(!panel.loading);
    }
}
