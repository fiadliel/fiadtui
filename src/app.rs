use crossterm::event;
use futures::Future;
use ratatui::Frame;

use crate::Message;

/// Application behavior.
pub trait App {
    type AppMessage;

    /// After every message, this function is called to draw the UI.
    ///
    /// As Ratatui is an immediate mode library, this very simply
    /// renders the whole UI on each call.
    ///
    /// The state is expected to be stored in the [`App`] implementation.
    fn draw(&mut self, frame: &mut Frame);

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
