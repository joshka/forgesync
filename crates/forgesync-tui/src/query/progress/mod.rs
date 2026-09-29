//! # Forward advisory operation progress to the terminal
//!
//! [`ProgressForwarder`] owns the engine-facing progress channel and its UI delivery task for one
//! writer generation. Operation scheduling gives the engine a sender, awaits execution, then calls
//! [`ProgressForwarder::finish`] before sending the terminal result. Buffered snapshots therefore
//! precede completion, while the engine's result remains the authoritative operation outcome.
//!
//! The engine uses nonblocking progress sends: a slow terminal may lose advisory snapshots but
//! cannot delay archive writes. This forwarder stops when the event loop closes its result channel.
//! Normal finishing closes the owner's sender and drains delivery; unexpected drop aborts the task
//! so a failed or abandoned operation cannot leak a receiver. Any sender clone held by execution
//! must be dropped before finishing, otherwise draining waits for that producer to close.
//!
//! This owner runs on the writer's active Tokio runtime. It does not interpret counters, mutate
//! panel state, choose cancellation policy, or derive success from a progress status.

use forgesync_engine::sync::SyncProgress;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::app::messages::QueryMessage;

/// Engine progress delivery and the task that forwards it for one admitted writer.
pub struct ProgressForwarder {
    /// Producer retained until execution has returned and buffered delivery can drain.
    sender: Option<mpsc::Sender<SyncProgress>>,
    /// Forwarding task retained through finishing so unexpected owner drop can abort it.
    task: Option<JoinHandle<()>>,
}

impl ProgressForwarder {
    /// Starts bounded progress delivery on the active writer runtime.
    ///
    /// # Panics
    ///
    /// Panics when called outside a Tokio runtime. Operation dispatch calls this inside its
    /// spawned writer task, so ordinary terminal execution already satisfies that requirement.
    pub fn start(generation: u64, destination: mpsc::Sender<QueryMessage>) -> Self {
        let (sender, receiver) = mpsc::channel(4);
        let task = tokio::spawn(forward_progress(generation, receiver, destination));
        Self {
            sender: Some(sender),
            task: Some(task),
        }
    }

    /// Gives execution its producer clone; execution must release it before finishing delivery.
    pub fn sender(&self) -> mpsc::Sender<SyncProgress> {
        self.sender
            .as_ref()
            .expect("sender exists until consuming finish")
            .clone()
    }

    /// Closes local production and waits for buffered progress before terminal result delivery.
    ///
    /// Delivery task failure is advisory and does not replace the writer's structured result.
    /// Retaining the handle during the await lets drop abort delivery if this future is cancelled.
    pub async fn finish(mut self) {
        self.sender.take();
        if let Some(task) = self.task.as_mut() {
            let _ = task.await;
        }
        self.task.take();
    }
}

impl Drop for ProgressForwarder {
    /// Stops delivery when writer execution or orderly finishing exits unexpectedly.
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

/// Forwards snapshots in arrival order and stops when the terminal no longer accepts messages.
async fn forward_progress(
    generation: u64,
    mut receiver: mpsc::Receiver<SyncProgress>,
    destination: mpsc::Sender<QueryMessage>,
) {
    while let Some(progress) = receiver.recv().await {
        let message = QueryMessage::OperationProgress {
            generation,
            progress,
        };
        if destination.send(message).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests;
