//! Simple async TUI wrapper based on ratatui, crossterm & tokio.
//!
//! This library provides a very simple event loop around ratatui's
//! abstractions.

mod app;
mod error;
mod message;

use std::{future::pending, io::Write, time::Duration};

use crossterm::{
    cursor,
    event::Event,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::{FutureExt, StreamExt};
use ratatui::{prelude::CrosstermBackend, CompletedFrame, Terminal};
use tokio::{
    select,
    sync::mpsc::{channel, Receiver, Sender},
    task::{JoinError, JoinSet},
    time::Instant,
};

pub use crate::app::App;
pub use crate::error::Error;
pub use crate::message::Message;
pub use crossterm::event;

#[derive(Debug)]
pub struct EventLoop<M, IO>
where
    M: Send + Sync + 'static,
    IO: Write,
{
    terminal: Terminal<CrosstermBackend<IO>>,
    should_quit: bool,
    should_suspend: bool,
    tx: Sender<Message<M>>,
    rx: Receiver<Message<M>>,
    joinset: JoinSet<Message<M>>,
}

impl<M, IO> EventLoop<M, IO>
where
    M: Send + Sync + 'static,
    IO: Write,
{
    pub fn new(io: IO) -> Result<EventLoop<M, IO>, Error<M>> {
        let (tx, rx) = channel(200);
        Self::with_channel(io, tx, rx)
    }

    pub fn with_channel(
        io: IO,
        sender: Sender<Message<M>>,
        receiver: Receiver<Message<M>>,
    ) -> Result<EventLoop<M, IO>, Error<M>> {
        let terminal = Terminal::new(CrosstermBackend::new(io))?;
        let joinset = JoinSet::new();
        let eventloop = EventLoop {
            terminal,
            should_quit: false,
            should_suspend: false,
            tx: sender,
            rx: receiver,
            joinset,
        };
        Ok(eventloop)
    }

    fn suspend(&mut self) -> Result<(), Error<M>> {
        self.should_suspend = false;
        self.exit()?;
        #[cfg(not(windows))]
        signal_hook::low_level::raise(signal_hook::consts::signal::SIGTSTP)?;
        Ok(())
    }

    fn resume(&mut self) -> Result<(), Error<M>> {
        self.enter()
    }

    fn enter(&mut self) -> Result<(), Error<M>> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            cursor::Hide
        )?;
        self.terminal.clear()?;
        Ok(())
    }

    pub(self) fn exit(&mut self) -> Result<(), Error<M>> {
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

    fn draw<A: App<AppMessage = M>>(&mut self, app: &mut A) -> Result<CompletedFrame, Error<M>> {
        let result = self.terminal.draw(|frame| app.draw(frame))?;
        Ok(result)
    }

    async fn handle_next_completed_future<A: App<AppMessage = M>>(
        &mut self,
        maybe_joined: Option<Result<Message<M>, JoinError>>,
    ) -> Result<(), Error<M>> {
        if let Some(Ok(message)) = maybe_joined {
            self.tx.send(message).await?;
        }

        Ok(())
    }

    async fn handle_next_completed_message<A: App<AppMessage = M>>(
        &mut self,
        maybe_message: Option<Message<M>>,
        app: &mut A,
        render_pending: &mut bool,
        last_render: &mut Instant,
        time_between_frames: &Duration,
    ) -> Result<(), Error<M>> {
        if let Some(message) = maybe_message {
            match message {
                Message::Quit => {
                    self.should_quit = true;
                }

                Message::Refresh => {
                    self.terminal.clear()?;
                    self.draw(app)?;
                    *render_pending = false;
                    *last_render = Instant::now();
                }

                Message::Suspend => {
                    self.should_suspend = true;
                }

                Message::App(m) => {
                    if let Some(fut) = app.handle_message(m) {
                        self.joinset.spawn(fut);
                    }

                    let now = Instant::now();

                    match now.checked_duration_since(*last_render) {
                        None => {
                            *last_render = now;
                        } // time went backwards?
                        Some(since_last) if !*render_pending => {
                            if since_last >= *time_between_frames {
                                self.draw(app)?;
                                *last_render = now;
                            } else {
                                let refresh_time = *last_render + *time_between_frames;
                                *render_pending = true;
                                self.joinset.spawn(async move {
                                    tokio::time::sleep_until(refresh_time).await;
                                    Message::Refresh
                                });
                            }
                        }
                        _ => {} // render pending, do nothing
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_next_completed_event<A: App<AppMessage = M>>(
        &mut self,
        maybe_event: Option<Result<Event, std::io::Error>>,
        app: &mut A,
    ) -> Result<(), Error<M>> {
        if let Some(Ok(event)) = maybe_event {
            match event {
                event::Event::Key(key) if key.kind == event::KeyEventKind::Press => {
                    match key.code {
                        event::KeyCode::Char('z')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            self.tx.send(Message::Suspend).await?;
                        }
                        event::KeyCode::Char('c')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            self.tx.send(Message::Quit).await?;
                        }
                        event::KeyCode::Char('r')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            self.tx.send(Message::Refresh).await?;
                        }
                        _ => {
                            if let Some(message) = app.handle_event(event) {
                                self.tx.send(message).await?;
                            }
                        }
                    }
                }
                _ => {
                    if let Some(message) = app.handle_event(event) {
                        self.tx.send(message).await?;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn event_loop<A: App<AppMessage = M>>(
        &mut self,
        mut app: A,
        frames_per_second: u8,
    ) -> Result<(), Error<M>> {
        assert!(frames_per_second > 0, "Refresh rate must be non-zero");

        let mut reader = event::EventStream::new();
        let mut last_render;
        let mut render_pending = false;
        let time_between_frames =
            Duration::from_millis(((frames_per_second as f64).recip() * 1000.0) as u64);

        self.enter()?;
        self.draw(&mut app)?;
        last_render = Instant::now();

        loop {
            if self.should_quit {
                break;
            }

            if self.should_suspend {
                self.suspend()?;
                self.resume()?;
                self.draw(&mut app)?;
            }

            select! {
                maybe_joined = Self::next_fut_message(&mut self.joinset).fuse() =>
                    self.handle_next_completed_future::<A>(maybe_joined).await?,
                maybe_message = self.rx.recv().fuse() =>
                    self.handle_next_completed_message::<A>(maybe_message, &mut app, &mut render_pending, &mut last_render, &time_between_frames).await?,
                maybe_event = reader.next().fuse() =>
                    self.handle_next_completed_event::<A>(maybe_event, &mut app).await?,
            }
        }

        self.exit()?;

        Ok(())
    }
}

impl<M, IO> Drop for EventLoop<M, IO>
where
    M: Send + Sync + 'static,
    IO: Write,
{
    fn drop(&mut self) {
        let _ = self.exit();
    }
}
