//! The terminal session loop: apply results, redraw when something changed, read input.

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
use crate::query::tasks::QueryTasks;
use crate::query::{QueryAction, QueryDispatch};
use crate::view;

/// How long to wait for input before checking for background results again.
const RESULT_POLL_INTERVAL: Duration = Duration::from_millis(40);

/// One terminal session. The launcher owns the runtime and tasks and closes the archive.
pub struct EventLoop<'a> {
    app: App,
    archive: Arc<Archive>,
    clients: Arc<HashMap<GitHubHost, GitHubClient>>,
    runtime: &'a Handle,
    tasks: &'a mut QueryTasks,
    sender: mpsc::Sender<QueryMessage>,
    receiver: mpsc::Receiver<QueryMessage>,
}

impl<'a> EventLoop<'a> {
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

    /// Runs until the app permits quitting. Frames are drawn only after input, a terminal event
    /// such as a resize, or a background message; nothing on screen animates on its own.
    pub fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> io::Result<()> {
        self.dispatch(self.app.initial_actions());
        let mut redraw = true;
        while !self.app.quit {
            redraw |= self.drain_messages();
            if redraw {
                terminal.draw(|frame| view::draw(frame, &mut self.app))?;
            }
            redraw = self.poll_input()?;
        }
        Ok(())
    }

    /// Starts each action with this session's resources.
    fn dispatch(&mut self, actions: impl IntoIterator<Item = QueryAction>) {
        let mut dispatch = QueryDispatch {
            archive: &self.archive,
            clients: &self.clients,
            runtime: self.runtime,
            sender: &self.sender,
            tasks: self.tasks,
        };
        for action in actions {
            dispatch.start(action, &mut self.app);
        }
    }

    /// Applies every pending message, refreshing views after a writer finishes. Returns whether
    /// any message arrived.
    fn drain_messages(&mut self) -> bool {
        let mut received = false;
        while let Ok(message) = self.receiver.try_recv() {
            received = true;
            let operation_finished = matches!(message, QueryMessage::OperationFinished(_));
            self.app.apply(message);
            if operation_finished {
                let actions = self.app.refresh_after_operation();
                self.dispatch(actions);
            }
        }
        received
    }

    /// Waits briefly for one terminal event and handles key presses and repeats. Returns whether
    /// an event arrived.
    fn poll_input(&mut self) -> io::Result<bool> {
        if !event::poll(RESULT_POLL_INTERVAL)? {
            return Ok(false);
        }
        if let Event::Key(key) = event::read()?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            let actions = self.app.handle_key(key);
            self.dispatch(actions);
        }
        Ok(true)
    }
}
