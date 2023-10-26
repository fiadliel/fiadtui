use std::{future::Future, io::stdout, pin::Pin};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use fiadtui::{App, EventLoop, Message};
use ratatui::widgets::Paragraph;

#[derive(Debug)]
enum CounterMessage {
    Increment,
    Decrement,
}

#[derive(Default)]
struct CounterApp {
    counter: i32,
}

impl App for CounterApp {
    type AppMessage = CounterMessage;

    fn draw(&self, frame: &mut ratatui::Frame) {
        frame.render_widget(Paragraph::new(self.counter.to_string()), frame.size());
    }

    fn handle_event(&self, event: crossterm::event::Event) -> Option<Message<CounterMessage>> {
        match event {
            crossterm::event::Event::Key(KeyEvent {
                code: KeyCode::Char('+'),
                kind: KeyEventKind::Press,
                ..
            }) => Some(Message::App(CounterMessage::Increment)),
            crossterm::event::Event::Key(KeyEvent {
                code: KeyCode::Char('-'),
                kind: KeyEventKind::Press,
                ..
            }) => Some(Message::App(CounterMessage::Decrement)),
            _ => None,
        }
    }

    // Need a concrete future type here, as `impl Future` can't be
    // inferred when no futures are used.
    fn handle_message(
        &mut self,
        message: Self::AppMessage,
    ) -> Option<Pin<Box<dyn Future<Output = Message<Self::AppMessage>> + Send + 'static>>> {
        match message {
            CounterMessage::Increment => {
                self.counter += 1;
                None
            }
            CounterMessage::Decrement => {
                self.counter -= 1;
                None
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let counter = CounterApp::default();
    let mut event_loop = EventLoop::new(stdout()).expect("Could not create event loop");

    event_loop
        .event_loop(counter)
        .await
        .expect("Error while running event loop");
}
