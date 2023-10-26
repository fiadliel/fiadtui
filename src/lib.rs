#![feature(return_position_impl_trait_in_trait)]

//! Simple async TUI wrapper based on ratatui, crossterm & tokio.
//!
//! This library provides a very simple event loop around ratatui's
//! abstractions.

use std::{
    future::{pending, Future},
    io::Write,
};

use crossterm::{
    cursor,
    event::{Event, EventStream, KeyCode, KeyEventKind},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::{FutureExt, StreamExt};
use ratatui::{prelude::CrosstermBackend, Frame, Terminal};
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
pub trait App<M> {
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
    fn handle_event(&self, event: Event) -> Option<Message<M>>;

    /// Handle messages.
    ///
    /// This function is called for every message. It may modify the
    /// state of the application, and (optionally) returns a future
    /// which will return a message in the future.
    fn handle_message(
        &mut self,
        message: M,
    ) -> Option<impl Future<Output = Message<M>> + Send + 'static>;
}

pub enum SystemMessage {
    Quit,
    Refresh,
    Suspend,
}

pub enum Message<M> {
    System(SystemMessage),
    App(M),
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

    pub async fn event_loop<A: App<M>>(&mut self, mut app: A) -> Result<(), M> {
        let mut reader = EventStream::new();

        self.enter()?;

        self.terminal.draw(|t| app.draw(t))?;

        loop {
            if self.should_quit {
                break;
            }

            if self.should_suspend {
                self.suspend()?;
                self.resume()?;
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
                      Message::System(SystemMessage::Quit) => {
                        self.should_quit = true;
                    },
                    Message::System(SystemMessage::Refresh) => {
                      self.terminal.clear()?;
                    },
                    Message::System(SystemMessage::Suspend) => {
                            self.should_suspend = true;
                        },
                        Message::App(m) => {
                            if let Some(fut) = app.handle_message(m) {
                              self.joinset.spawn(fut);
                            }
                            self.terminal.draw(|t| app.draw(t))?;
                        }
                    }
                } else {
                  break;
                }
              },
              maybe_event = reader.next().fuse() => {
                if let Some(Ok(event)) = maybe_event {
                    match event {
                      Event::Key(key) if key.kind == KeyEventKind::Press => {
                        match key.code {
                          KeyCode::Char('z') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            self.tx.send(Message::System(SystemMessage::Suspend))?;
                          },
                          KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            self.tx.send(Message::System(SystemMessage::Quit))?;
                          },
                          KeyCode::Char('r') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            self.tx.send(Message::System(SystemMessage::Refresh))?;
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
