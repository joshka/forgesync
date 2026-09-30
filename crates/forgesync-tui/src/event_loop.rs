//! # Coordinate terminal input and background results
//!
//! `EventLoop` owns the live app and message channel for one terminal session. It borrows the
//! runtime and tracked tasks supplied by the launcher, and shares the already opened archive and
//! configured provider clients with query workers. No archive lifecycle or runtime is created here.
//!
//! Each iteration drains completed messages, draws current state, and polls input. Completing an
//! operation requests the app's refresh actions before the next draw. Input dispatch is limited to
//! key presses and repeats; other terminal events retain the existing polling behavior.
//!
//! Query dispatch updates pending panel state synchronously and starts background work. Generation
//! checks and cancellation policy remain with app/query owners. The launcher restores the terminal,
//! then awaits task cleanup and closes the archive; exiting this loop does not abandon those tasks.

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use forgesync_core::identity::GitHubHost;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use tokio::runtime::Handle;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::messages::QueryMessage;
use crate::query::requests::QueryAction;
use crate::query::start_query;
use crate::query::tasks::QueryTasks;
use crate::view;

/// Session state and dispatch resources shared by input and completed background messages.
pub struct EventLoop<'a> {
    /// Navigation, cached projections, and active operation display.
    app: App,
    /// Already opened archive shared with background reads and writes.
    archive: Arc<Archive>,
    /// Explicit host-keyed provider clients for acquisition actions.
    clients: Arc<HashMap<GitHubHost, GitHubClient>>,
    /// Active runtime used to spawn asynchronous archive work.
    runtime: &'a Handle,
    /// Launcher-owned handles retained for orderly shutdown after terminal restoration.
    tasks: &'a mut QueryTasks,
    /// Completion destination cloned into background work.
    sender: mpsc::Sender<QueryMessage>,
    /// Session-owned completed-result stream drained before each frame.
    receiver: mpsc::Receiver<QueryMessage>,
}

impl<'a> EventLoop<'a> {
    /// Binds the explicit execution resources to a fresh app and bounded completion channel.
    ///
    /// The four arguments are the session's execution resources; their lifecycle remains with the
    /// launcher. Construction itself performs no query and does not install terminal state.
    pub fn new(
        archive: Arc<Archive>,
        clients: Arc<HashMap<GitHubHost, GitHubClient>>,
        runtime: &'a Handle,
        tasks: &'a mut QueryTasks,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(16);
        Self {
            app: App::default(),
            archive,
            clients,
            runtime,
            tasks,
            sender,
            receiver,
        }
    }

    /// Loads initial panels and keeps input responsive until the app permits quitting.
    pub fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> io::Result<()> {
        self.dispatch(self.app.initial_actions());
        while !self.app.quit {
            self.drain_messages();
            terminal.draw(|frame| view::draw(frame, &mut self.app))?;
            self.poll_input()?;
        }
        Ok(())
    }

    /// Starts each explicit action using the same session resources and tracked-task owner.
    fn dispatch(&mut self, actions: impl IntoIterator<Item = QueryAction>) {
        for action in actions {
            start_query(
                action,
                &mut self.app,
                &self.archive,
                &self.clients,
                self.runtime,
                &self.sender,
                self.tasks,
            );
        }
    }

    /// Applies available results before drawing, refreshing panels after operation completion.
    fn drain_messages(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            let operation_finished = matches!(message, QueryMessage::OperationFinished { .. });
            self.app.apply(message);
            if operation_finished {
                let actions = self.app.refresh_after_operation();
                self.dispatch(actions);
            }
        }
    }

    /// Polls briefly and dispatches presses/repeats, preserving other event handling behavior.
    fn poll_input(&mut self) -> io::Result<()> {
        if event::poll(Duration::from_millis(40))?
            && let Event::Key(key) = event::read()?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            let actions = self.app.handle_key(key);
            self.dispatch(actions);
        }
        Ok(())
    }
}
