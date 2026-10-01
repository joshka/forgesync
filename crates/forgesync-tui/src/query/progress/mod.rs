//! Forwards engine progress to the app for the running writer.
//!
//! The engine sends progress without blocking, so a slow terminal loses snapshots rather than
//! delaying archive writes. [`ProgressForwarder::finish`] drains buffered snapshots before the
//! writer sends its result, which keeps progress ordered before completion. Every sender clone
//! must be dropped before finishing, or draining waits for it.

use forgesync_engine::sync::SyncProgress;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::app::messages::QueryMessage;

pub struct ProgressForwarder {
    sender: Option<mpsc::Sender<SyncProgress>>,
    /// Kept while finishing so dropping an unfinished forwarder aborts delivery.
    task: Option<JoinHandle<()>>,
}

impl ProgressForwarder {
    /// # Panics
    ///
    /// Panics outside a Tokio runtime.
    pub fn start(destination: mpsc::Sender<QueryMessage>) -> Self {
        let (sender, receiver) = mpsc::channel(4);
        let task = tokio::spawn(forward_progress(receiver, destination));
        Self {
            sender: Some(sender),
            task: Some(task),
        }
    }

    pub fn sender(&self) -> mpsc::Sender<SyncProgress> {
        self.sender
            .as_ref()
            .expect("sender exists until consuming finish")
            .clone()
    }

    /// Closes production and waits for buffered progress to be delivered.
    pub async fn finish(mut self) {
        self.sender.take();
        if let Some(task) = self.task.as_mut() {
            let _ = task.await;
        }
        self.task.take();
    }
}

impl Drop for ProgressForwarder {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

async fn forward_progress(
    mut receiver: mpsc::Receiver<SyncProgress>,
    destination: mpsc::Sender<QueryMessage>,
) {
    while let Some(progress) = receiver.recv().await {
        if destination
            .send(QueryMessage::OperationProgress(progress))
            .await
            .is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests;
