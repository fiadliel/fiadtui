use std::{future::Future, io::stdout, pin::Pin, time::Duration};

use fiadtui::{event::Event, App, EventLoop, Message};
use futures::{executor::block_on, future::join_all};
use ratatui::widgets::Paragraph;
use tokio::{
    sync::mpsc::channel,
    task::{self, spawn_blocking},
    time,
};

#[derive(Debug)]
enum TickMessage {
    Tick,
}

#[derive(Default)]
struct TickApp {
    counter: i32,
}

impl App for TickApp {
    type AppMessage = TickMessage;

    fn draw(&mut self, frame: &mut ratatui::Frame) {
        frame.render_widget(Paragraph::new(self.counter.to_string()), frame.size());
    }

    fn handle_event(&self, _event: Event) -> Option<Message<TickMessage>> {
        None
    }

    // Need a concrete future type here, as `impl Future` can't be
    // inferred when no futures are used.
    fn handle_message(
        &mut self,
        message: Self::AppMessage,
    ) -> Option<Pin<Box<dyn Future<Output = Message<Self::AppMessage>> + Send + 'static>>> {
        match message {
            TickMessage::Tick => {
                self.counter += 1;
                None
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (tx, rx) = channel(250);
    let counter = TickApp::default();
    let mut event_loop =
        EventLoop::with_channel(stdout(), tx.clone(), rx).expect("Could not create event loop");

    let evt = spawn_blocking(move || block_on(event_loop.event_loop(counter, 60)));

    let forever = task::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(500));

        loop {
            interval.tick().await;
            if !tx.send(Message::App(TickMessage::Tick)).await.is_ok() {
                break;
            }
        }
        Ok(())
    });

    join_all(vec![evt, forever]).await;
}
