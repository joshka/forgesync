//! Background task handles, kept for orderly shutdown.

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Read tasks are aborted on shutdown because they hold no durable state. The writer is cancelled
/// cooperatively and awaited by [`Self::stop`] so the engine can release its lease; dropping
/// without `stop` only signals cancellation.
#[derive(Default)]
pub struct QueryTasks {
    handles: Vec<JoinHandle<()>>,
    /// Most recent writer, kept after it finishes until replaced or shut down.
    operation: Option<ActiveOperation>,
}

struct ActiveOperation {
    handle: JoinHandle<()>,
    cancellation: CancellationToken,
}

impl QueryTasks {
    /// Tracks a read, pruning finished ones.
    pub fn push(&mut self, handle: JoinHandle<()>) {
        self.handles.retain(|task| !task.is_finished());
        self.handles.push(handle);
    }

    /// Tracks a newly admitted writer. The previous one has already reported its result.
    pub fn track_operation(&mut self, handle: JoinHandle<()>, cancellation: CancellationToken) {
        self.operation = Some(ActiveOperation {
            handle,
            cancellation,
        });
    }

    /// Repeated requests are harmless.
    pub fn cancel_operation(&self) {
        if let Some(operation) = &self.operation {
            operation.cancellation.cancel();
        }
    }

    /// Aborts reads, then cancels the writer and waits for it to release its lease.
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
