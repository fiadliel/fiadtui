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
