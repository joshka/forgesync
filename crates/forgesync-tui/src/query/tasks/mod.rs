//! # Retain and stop background terminal work
//!
//! [`QueryTasks`] owns read handles and the most recently started writer. Read starters register
//! short-lived archive reads through [`QueryTasks::push`]; operation dispatch registers the writer
//! and its cancellation token through [`QueryTasks::track_operation`]. The app's writer display
//! admits one operation at a time, while this module owns task cleanup rather than UI state.
//!
//! The event loop calls [`QueryTasks::stop`] before returning. Reads are aborted and drained;
//! the writer receives cooperative cancellation and is awaited so engine cleanup can finish.
//! Dropping the owner also aborts reads and signals the writer, but cannot wait for its cleanup.
//! Task results and panic details are not presented here: normal results reach the app through
//! typed messages, and this owner exists to manage lifetime rather than interpret outcomes.

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Owns background reads and the current writer until explicit shutdown.
///
/// Reads can be aborted because they hold no durable workflow state. Writers instead receive
/// cooperative cancellation and are awaited by [`Self::stop`] so engine cleanup can release leases.
/// Dropping this owner is a fallback: it signals cancellation but cannot await asynchronous
/// cleanup.
#[derive(Default)]
pub struct QueryTasks {
    /// Read handles retained until the next read starts or shutdown drains them.
    handles: Vec<JoinHandle<()>>,
    /// Most recently started writer, retained even after it finishes until replacement or
    /// shutdown.
    operation: Option<ActiveOperation>,
}

/// Couples a writer task with the token used to request its engine cleanup.
struct ActiveOperation {
    /// Completion handle awaited during orderly terminal shutdown.
    handle: JoinHandle<()>,
    /// Cooperative cancellation shared with the running engine operation.
    cancellation: CancellationToken,
}

impl QueryTasks {
    /// Tracks a read task after pruning handles for completed reads.
    pub fn push(&mut self, handle: JoinHandle<()>) {
        self.handles.retain(|task| !task.is_finished());
        self.handles.push(handle);
    }

    /// Tracks the writer admitted by the app's operation display.
    ///
    /// The caller must only replace a writer that has reported its terminal result; the display
    /// rejects overlapping engine work. The previous task may still be returning from delivery.
    /// Retaining the handle here lets shutdown wait for lease cleanup after cancellation.
    pub fn track_operation(&mut self, handle: JoinHandle<()>, cancellation: CancellationToken) {
        self.operation = Some(ActiveOperation {
            handle,
            cancellation,
        });
    }

    /// Requests cancellation of the tracked writer; repeated requests are harmless.
    pub fn cancel_operation(&self) {
        if let Some(operation) = &self.operation {
            operation.cancellation.cancel();
        }
    }

    /// Aborts outstanding reads and requests cancellation of the active writer before shutdown.
    /// The writer is awaited so it can release its archive lease.
    pub async fn stop(&mut self) {
        for task in self.handles.drain(..) {
            task.abort();
            let _ = task.await;
        }
        if let Some(operation) = self.operation.take() {
            operation.cancellation.cancel();
            let _ = operation.handle.await;
        }
    }
}

impl Drop for QueryTasks {
    /// Aborts reads and signals the writer when orderly asynchronous shutdown was skipped.
    fn drop(&mut self) {
        for task in &self.handles {
            task.abort();
        }
        if let Some(operation) = &self.operation {
            operation.cancellation.cancel();
        }
    }
}

#[cfg(test)]
mod tests;
