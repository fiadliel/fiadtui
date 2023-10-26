//! Simple async TUI wrapper based on ratatui, crossterm & tokio.
//!
//! This library provides a very simple event loop around ratatui's
//! abstractions.

use std::{
    future::{pending, Future},
    io::Write,
};

pub use crossterm::event;

use crossterm::{
    cursor,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::{FutureExt, StreamExt};
use ratatui::{prelude::CrosstermBackend, CompletedFrame, Frame, Terminal};
use thiserror::Error as ThisError;
use tokio::{
    select,
    sync::mpsc::{error::SendError, unbounded_channel, UnboundedReceiver, UnboundedSender},
    task::{JoinError, JoinSet},
};

#[derive(ThisError, Debug)]
pub enum Error<M> {
    #[error("IO error")]
    IO(#[from] std::io::Error),
    #[error("Channel send error")]
    Send(#[from] SendError<Message<M>>),
}

/// Type used for results returned by library.
type Result<A, M> = std::result::Result<A, Error<M>>;

/// Application behavior.
pub trait App {
    type AppMessage;

    /// After every message, this function is called to draw the UI.
    ///
    /// As Ratatui is an immediate mode library, this very simply
    /// renders the whole UI on each call.
    ///
    /// The state is expected to be stored in the [`App`] implementation.
    fn draw(&self, frame: &mut Frame);

    /// Handle events.
    ///
    /// This function is called for every event. It maps the event
    /// to an (optional) message which may be handled by the application.
    fn handle_event(&self, event: event::Event) -> Option<Message<Self::AppMessage>>;

    /// Handle messages.
    ///
    /// This function is called for every message. It may modify the
    /// state of the application, and (optionally) returns a future
    /// which will return a message in the future.
    fn handle_message(
        &mut self,
        message: Self::AppMessage,
    ) -> Option<impl Future<Output = Message<Self::AppMessage>> + Send + 'static>;
}

#[non_exhaustive]
pub enum Message<M> {
    Quit,
    Refresh,
    Suspend,
    App(M),
}

impl<M> From<M> for Message<M> {
    fn from(value: M) -> Self {
        Message::App(value)
    }
}

#[derive(Debug)]
pub struct EventLoop<M, IO: Write> {
    terminal: Terminal<CrosstermBackend<IO>>,
    should_quit: bool,
    should_suspend: bool,
    tx: UnboundedSender<Message<M>>,
    rx: UnboundedReceiver<Message<M>>,
    joinset: JoinSet<Message<M>>,
}

impl<M, IO> EventLoop<M, IO>
where
    M: Send + Sync + 'static,
    IO: Write,
{
    pub fn new(io: IO) -> Result<EventLoop<M, IO>, M> {
        let terminal = Terminal::new(CrosstermBackend::new(io))?;
        let (tx, rx) = unbounded_channel();
        let joinset = JoinSet::new();
        let eventloop = EventLoop {
            terminal,
            should_quit: false,
            should_suspend: false,
            tx,
            rx,
            joinset,
        };
        Ok(eventloop)
    }

    fn suspend(&mut self) -> Result<(), M> {
        self.should_suspend = false;
        self.exit()?;
        #[cfg(not(windows))]
        signal_hook::low_level::raise(signal_hook::consts::signal::SIGTSTP)?;
        Ok(())
    }

    fn resume(&mut self) -> Result<(), M> {
        self.enter()
    }

    fn enter(&mut self) -> Result<(), M> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            cursor::Hide
        )?;
        self.terminal.clear()?;
        Ok(())
    }

    fn exit(&mut self) -> Result<(), M> {
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            cursor::Show
        )?;
        Ok(())
    }

    async fn next_fut_message(
        joinset: &mut JoinSet<Message<M>>,
    ) -> Option<std::result::Result<Message<M>, JoinError>> {
        if joinset.is_empty() {
            pending().await
        } else {
            joinset.join_next().await
        }
    }

    fn draw<A: App<AppMessage = M>>(&mut self, app: &A) -> Result<CompletedFrame, M> {
        let result = self.terminal.draw(|t| app.draw(t))?;
        Ok(result)
    }

    pub async fn event_loop<A: App<AppMessage = M>>(&mut self, mut app: A) -> Result<(), M> {
        let mut reader = event::EventStream::new();

        self.enter()?;
        self.draw(&app)?;

        loop {
            if self.should_quit {
                break;
            }

            if self.should_suspend {
                self.suspend()?;
                self.resume()?;
                self.draw(&app)?;
            }

            select! {
              maybe_joined = Self::next_fut_message(&mut self.joinset).fuse() => {
                  if let Some(Ok(message)) = maybe_joined {
                    self.tx.send(message)?;
                  }
              },
              maybe_message = self.rx.recv().fuse() => {
                if let Some(message) = maybe_message {
                    match message {
                      Message::Quit => {
                        self.should_quit = true;
                    },
                    Message::Refresh => {
                      self.terminal.clear()?;
                      self.draw(&app)?;
                    },
                    Message::Suspend => {
                      self.should_suspend = true;
                    },
                    Message::App(m) => {
                      if let Some(fut) = app.handle_message(m) {
                        self.joinset.spawn(fut);
                      }

                      self.draw(&app)?;
                    }
                  }
                } else {
                  break;
                }
              },
              maybe_event = reader.next().fuse() => {
                if let Some(Ok(event)) = maybe_event {
                    match event {
                      event::Event::Key(key) if key.kind == event::KeyEventKind::Press => {
                        match key.code {
                          event::KeyCode::Char('z') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                            self.tx.send(Message::Suspend)?;
                          },
                          event::KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                            self.tx.send(Message::Quit)?;
                          },
                          event::KeyCode::Char('r') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                            self.tx.send(Message::Refresh)?;
                          },
                          _ => { if let Some(message) = app.handle_event(event) {
                            self.tx.send(message)?;
                          }}
                        }
                      },
                      _ => {
                          if let Some(message) = app.handle_event(event) {
                            self.tx.send(message)?;
                          }
                      },
                    }
                } else {
                    break;
                }
              }
            }
        }

        self.exit()?;

        Ok(())
    }
}
