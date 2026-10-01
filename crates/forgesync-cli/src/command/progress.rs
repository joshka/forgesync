//! # Human acquisition progress at the process boundary
//!
//! [`ProgressReporter`] owns the bounded channel and terminal task shared by sync and retry.
//! Commands hand the engine a sender, await acquisition, then finish reporting before rendering
//! their final result. JSON commands do not install a reporter. Human output includes a startup
//! spinner on interactive stderr and periodic plain snapshots when redirected. Tracing shares the
//! spinner writer; verbose output also prints every delivered event above the display.
//!
//! Progress is advisory: the engine uses nonblocking sends, so a slow terminal cannot hold up
//! archive work. Finishing closes this owner's sender and drains buffered events; dropping the
//! owner aborts the terminal task if the command exits unexpectedly. No report or exit status is
//! derived from progress events: the engine's terminal result remains authoritative.

use std::io::IsTerminal;
use std::time::Duration;

use forgesync_engine::sync::SyncProgress;
use indicatif::ProgressStyle;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::Span;
use tracing_indicatif::span_ext::IndicatifSpanExt;
use tracing_indicatif::suspend_tracing_indicatif;
use tracing_indicatif::writer::get_indicatif_stderr_writer;

use crate::OutputMode;

/// Owns optional progress delivery for one acquisition command.
pub struct ProgressReporter {
    /// Sender retained until acquisition ends, then closed before waiting for the receiver.
    sender: Option<mpsc::Sender<SyncProgress>>,
    /// Human stderr renderer; absent when process output policy suppresses progress.
    task: Option<JoinHandle<()>>,
}

impl ProgressReporter {
    /// Starts bounded stderr reporting for human output without changing result policy.
    pub fn start(command: &'static str, output: OutputMode, verbose: u8) -> Self {
        if output.is_json() {
            return Self {
                sender: None,
                task: None,
            };
        }
        let display = ProgressDisplay::start(command);
        let (sender, receiver) = mpsc::channel(4);
        let task = tokio::spawn(report_progress(command, receiver, verbose, display));
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

/// Updates the terminal display as events arrive; plain output coalesces every two seconds.
/// Plain waits continue before the first event. Closing delivery drains the final snapshot and
/// drops the display span, clearing animation before the caller renders its authoritative result.
async fn report_progress(
    command: &str,
    mut receiver: mpsc::Receiver<SyncProgress>,
    verbose: u8,
    display: ProgressDisplay,
) {
    let started = tokio::time::Instant::now();
    let mut ticks =
        tokio::time::interval_at(started + Duration::from_secs(2), Duration::from_secs(2));
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut latest = None;
    loop {
        tokio::select! {
            event = receiver.recv() => {
                let Some(progress) = event else {
                    if verbose == 0 && let Some(progress) = latest {
                        display.snapshot(command, &progress, verbose);
                    }
                    break;
                };
                if display.is_terminal() || verbose > 0 || latest.is_none() {
                    display.snapshot(command, &progress, verbose);
                }
                latest = Some(progress);
            }
            _ = ticks.tick() => {
                if !display.is_terminal() {
                    eprintln!(
                        "forgesync: {command}: still running ({}s elapsed; waiting for acquisition to finish)",
                        started.elapsed().as_secs()
                    );
                    if let Some(progress) = &latest {
                        display.snapshot(command, progress, verbose);
                    }
                }
            }
        }
    }
}

/// One acquisition display. Dropping its span clears the spinner, including task abortion.
/// The subscriber owns animation ticks and coordinates its stderr writer with diagnostics.
enum ProgressDisplay {
    Plain,
    Terminal(Span),
}

impl ProgressDisplay {
    /// Starts before credential discovery. Without the process's terminal layer, plain reporting
    /// remains available, including when subscriber installation failed or JSON logs were selected.
    fn start(command: &str) -> Self {
        let message = format!("forgesync: {command}: preparing acquisition (Ctrl-C to cancel)");
        if std::io::stderr().is_terminal() && get_indicatif_stderr_writer().is_some() {
            let span = tracing::info_span!(
                parent: None,
                "acquisition",
                indicatif.pb_show = tracing::field::Empty
            );
            let style = ProgressStyle::with_template("{spinner} [{elapsed_precise}] {msg}")
                .expect("valid acquisition progress template");
            span.pb_set_style(&style);
            span.pb_set_message(&message);
            span.pb_start();
            Self::Terminal(span)
        } else {
            eprintln!("{message}");
            Self::Plain
        }
    }

    /// Selects in-place updates rather than periodic appended lines.
    fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminal(_))
    }

    /// Updates counters without claiming that elapsed animation means provider work completed.
    /// Verbose terminal events use the layer's suspension boundary to preserve the active display.
    fn snapshot(&self, command: &str, progress: &SyncProgress, verbose: u8) {
        let message = progress_message(command, progress);
        match self {
            Self::Terminal(span) => {
                span.pb_set_message(&message);
                if verbose > 0 {
                    suspend_tracing_indicatif(|| eprintln!("{message}"));
                }
            }
            Self::Plain => eprintln!("{message}"),
        }
    }
}

/// Formats only bounded counters and workflow state, never discussion content or credentials.
fn progress_message(command: &str, progress: &SyncProgress) -> String {
    let repository = progress.repository.as_deref().unwrap_or(command);
    format!(
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
    )
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
    #[case::default_json(OutputMode::Json, 0)]
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
        let reporter = ProgressReporter::start("retry", OutputMode::Text, 0);
        assert!(reporter.sender().is_some());
        timeout(Duration::from_secs(5), reporter.finish())
            .await
            .expect("reporter drains after local sender closes");
    }

    #[tokio::test]
    async fn dropping_the_owner_aborts_the_receiver_task() {
        let reporter = ProgressReporter::start("sync", OutputMode::Text, 0);
        let sender = reporter.sender().expect("human progress channel");
        drop(reporter);
        timeout(Duration::from_secs(5), sender.closed())
            .await
            .expect("aborted receiver closes delivery");
    }
}
