//! # Human acquisition progress at the process boundary
//!
//! [`ProgressReporter`] owns the bounded channel and terminal task shared by sync and retry.
//! Commands hand the engine a sender, await acquisition, then finish reporting before rendering
//! their final result. JSON and quiet commands do not install a reporter.
//!
//! Progress is advisory: the engine uses nonblocking sends, so a slow terminal cannot hold up
//! archive work. Finishing closes this owner's sender and drains buffered events; dropping the
//! owner aborts the terminal task if the command exits unexpectedly. No report or exit status is
//! derived from progress events: the engine's terminal result remains authoritative.

use forgesync_engine::sync::SyncProgress;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::OutputMode;

/// Owns optional progress delivery for one acquisition command.
pub struct ProgressReporter {
    /// Sender retained until acquisition ends, then closed before waiting for the receiver.
    sender: Option<mpsc::Sender<SyncProgress>>,
    /// Human stderr renderer; absent when process output policy suppresses progress.
    task: Option<JoinHandle<()>>,
}

impl ProgressReporter {
    /// Starts bounded stderr reporting for verbose human output without changing result policy.
    pub fn start(command: &'static str, output: OutputMode, verbose: u8) -> Self {
        if verbose == 0 || output.is_json() {
            return Self {
                sender: None,
                task: None,
            };
        }
        let (sender, receiver) = mpsc::channel(4);
        let task = tokio::spawn(report_progress(command, receiver));
        Self {
            sender: Some(sender),
            task: Some(task),
        }
    }

    /// Supplies optional advisory delivery; the engine must release its sender when work ends.
    pub fn sender(&self) -> Option<mpsc::Sender<SyncProgress>> {
        self.sender.clone()
    }

    /// Closes local delivery and waits for buffered events before the final result is printed.
    ///
    /// Call this after the engine future returns and releases its senders. A failed renderer does
    /// not replace the acquisition result. `Drop` still aborts the task if this wait is cancelled.
    pub async fn finish(mut self) {
        self.sender.take();
        if let Some(task) = self.task.as_mut() {
            let _ = task.await;
        }
        self.task.take();
    }
}

impl Drop for ProgressReporter {
    /// Aborts a retained stderr renderer on early owner exit so delivery cannot outlive its
    /// command.
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

/// Drains snapshots in arrival order using the command name when no repository is selected.
async fn report_progress(command: &str, mut receiver: mpsc::Receiver<SyncProgress>) {
    while let Some(progress) = receiver.recv().await {
        render_progress(command, &progress);
    }
}

/// Prints only bounded counters and workflow state, never discussion content or credentials.
fn render_progress(command: &str, progress: &SyncProgress) {
    let repository = progress.repository.as_deref().unwrap_or(command);
    eprintln!(
        "forgesync: {}: {:?}, {}/{} jobs, {} threads, {} comments, {} PRs, {} reviews, {} review threads",
        repository,
        progress.status,
        progress.completed_jobs,
        progress.total_jobs,
        progress.threads_seen,
        progress.comments_seen,
        progress.pull_request_metadata_seen,
        progress.reviews_seen,
        progress.review_threads_seen
    );
}

#[cfg(test)]
mod tests {
    //! # Progress task lifetime and output-policy selection
    //!
    //! Named output/verbosity cases verify when advisory delivery is absent.
    //! The async cases create a real reporter task but no engine workflow or progress payloads.
    //! Finishing must close the retained sender so an empty receiver can terminate.
    //!
    //! Dropping the owner must close delivery through task abortion; a bounded wait observes that
    //! channel effect without reaching into task internals. These cases establish lifecycle rather
    //! than buffered-event wording or engine outcome. Process/report suites cover those separately.
    //! All test operations are explicit and use their own reporter/channel state.

    use std::time::Duration;

    use tokio::time::timeout;

    use super::ProgressReporter;
    use crate::OutputMode;

    #[rstest::rstest]
    #[case::quiet_text(OutputMode::Text, 0)]
    #[case::quiet_json(OutputMode::Json, 0)]
    #[case::verbose_json(OutputMode::Json, 1)]
    fn suppressed_progress_has_no_delivery_channel(
        #[case] output: OutputMode,
        #[case] verbose: u8,
    ) {
        let reporter = ProgressReporter::start("sync", output, verbose);
        assert!(reporter.sender().is_none());
    }

    #[tokio::test]
    async fn finish_closes_the_owned_sender_before_waiting_for_the_receiver() {
        let reporter = ProgressReporter::start("retry", OutputMode::Text, 1);
        assert!(reporter.sender().is_some());
        timeout(Duration::from_secs(5), reporter.finish())
            .await
            .expect("reporter drains after local sender closes");
    }

    #[tokio::test]
    async fn dropping_the_owner_aborts_the_receiver_task() {
        let reporter = ProgressReporter::start("sync", OutputMode::Text, 1);
        let sender = reporter.sender().expect("verbose human progress channel");
        drop(reporter);
        timeout(Duration::from_secs(5), sender.closed())
            .await
            .expect("aborted receiver closes delivery");
    }
}
