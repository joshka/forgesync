//! # Display state for the single active archive writer
//!
//! [`OperationDisplay`] owns the current writer generation and its transient presentation state.
//! [`OperationState`] distinguishes idle from running work, keeping the label and optional
//! progress together only while a writer is active. The query task owns execution and cancellation;
//! this module owns which of its messages are current enough to display.
//!
//! Beginning work reserves a generation, but refuses a second writer while the first is running.
//! Progress is accepted only for that active generation. Completion clears transient presentation
//! and returns a status line to the app; it never decides what archive data should be refreshed.
//!
//! Input uses `busy` to request cancellation before quitting, and the view reads `label` and
//! `progress`. Read requests remain independently usable while the writer is running. Late progress
//! from completed or replaced work cannot make the terminal appear busy again.

use forgesync_engine::sync::SyncProgress;

/// Current writer identity and the presentation state of that generation.
#[derive(Debug, Default)]
pub struct OperationDisplay {
    /// Most recently reserved writer generation, retained after completion to reject older work.
    pub generation: u64,
    /// Running presentation or idle; execution and cancellation live in query task ownership.
    pub state: OperationState,
}

/// Transient presentation that exists only while an archive writer is running.
#[derive(Debug, Default)]
pub enum OperationState {
    /// No writer is active and no transient progress is retained.
    #[default]
    Idle,
    /// One writer is active, with its label and latest advisory snapshot.
    Running {
        /// User-facing operation name shown even before the first progress snapshot arrives.
        label: String,
        /// Latest counters; absent until the engine sends its first current snapshot.
        progress: Option<SyncProgress>,
    },
}

impl OperationDisplay {
    /// Reserves the writer slot, leaving an existing active operation completely unchanged.
    pub fn begin(&mut self, label: &str) -> Option<u64> {
        if self.busy() {
            return None;
        }
        self.generation += 1;
        self.state = OperationState::Running {
            label: label.to_owned(),
            progress: None,
        };
        Some(self.generation)
    }

    /// Reports whether input should cancel a writer before allowing the terminal to quit.
    pub fn busy(&self) -> bool {
        matches!(self.state, OperationState::Running { .. })
    }

    /// Returns the active operation name for presentation, absent while idle.
    pub fn label(&self) -> Option<&str> {
        match &self.state {
            OperationState::Idle => None,
            OperationState::Running { label, .. } => Some(label),
        }
    }

    /// Returns the latest active-generation counters without affecting workflow state.
    pub fn progress(&self) -> Option<&SyncProgress> {
        match &self.state {
            OperationState::Idle => None,
            OperationState::Running { progress, .. } => progress.as_ref(),
        }
    }

    /// Accepts a snapshot only while its generation is the currently running writer.
    pub fn update_progress(&mut self, generation: u64, snapshot: SyncProgress) {
        if generation != self.generation {
            return;
        }
        if let OperationState::Running { progress, .. } = &mut self.state {
            *progress = Some(snapshot);
        }
    }

    /// Clears current transient state and returns the terminal status, ignoring older completions.
    ///
    /// The generation remains reserved after completion, so late progress cannot resurrect it.
    /// The app owns the returned status and any subsequent archive refresh requests.
    pub fn finish(&mut self, generation: u64, result: Result<String, String>) -> Option<String> {
        if generation != self.generation {
            return None;
        }
        self.state = OperationState::Idle;
        Some(match result {
            Ok(summary) => summary,
            Err(error) => format!("Failed: {error}"),
        })
    }
}

#[cfg(test)]
mod tests;
