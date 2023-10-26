use std::{future::Future, io::stdout, pin::Pin, time::Duration};

use fiadtui::{
    event::{Event, KeyCode, KeyEvent, KeyEventKind},
    App, EventLoop, Message,
};
use ratatui::widgets::Paragraph;

#[derive(Debug)]
enum CounterMessage {
    Increment,
    Decrement,
    DelayedUpdate(i32),
}

#[derive(Default)]
struct CounterApp {
    pending_update: bool,
    counter: i32,
}

impl App for CounterApp {
    type AppMessage = CounterMessage;

    fn draw(&self, frame: &mut ratatui::Frame) {
        frame.render_widget(Paragraph::new(self.counter.to_string()), frame.size());
    }

    fn handle_event(&self, event: Event) -> Option<Message<CounterMessage>> {
        match event {
            Event::Key(KeyEvent {
                code: KeyCode::Char('+'),
                kind: KeyEventKind::Press,
                ..
            }) => Some(CounterMessage::Increment.into()),
            Event::Key(KeyEvent {
                code: KeyCode::Char('-'),
                kind: KeyEventKind::Press,
                ..
            }) => Some(CounterMessage::Decrement.into()),
            _ => None,
        }
    }

    // Need a concrete future type here, as multiple async blocks
    // have different types.
    fn handle_message(
        &mut self,
        message: Self::AppMessage,
    ) -> Option<Pin<Box<dyn Future<Output = Message<Self::AppMessage>> + Send + 'static>>> {
        match message {
            CounterMessage::Increment => {
                if self.pending_update {
                    return None;
                }

                let future_value = self.counter + 1;
                self.pending_update = true;
                Some(Box::pin(async move {
                    let _ = tokio::time::sleep(Duration::from_secs(1)).await;
                    CounterMessage::DelayedUpdate(future_value).into()
                }))
            }
            CounterMessage::Decrement => {
                if self.pending_update {
                    return None;
                }

                let future_value = self.counter - 1;
                self.pending_update = true;
                Some(Box::pin(async move {
                    let _ = tokio::time::sleep(Duration::from_secs(1)).await;
                    CounterMessage::DelayedUpdate(future_value).into()
                }))
            }
            CounterMessage::DelayedUpdate(v) => {
                self.pending_update = false;
                self.counter = v;
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
